//! Settlement templates.
//!
//! Settlement pages deliberately keep the same ownership model: services and
//! settlement-owned information on the left, service context in the center,
//! and the active player's party on the right.

use adventuresim_core::{
    activity::{PRAYER_MORALE_LIMIT, PRAYER_MORALE_SCALE_MINUTES, settlement_population_scale},
    prelude::Skill,
    strategic_schedule::{
        BASELINE_FATIGUE_PER_DAY, DailySchedule, FATIGUE_RESERVOIR_PER_PREVIEW_POINT,
        LABOR_FATIGUE_PER_HOUR, LEISURE_FATIGUE_RECOVERY_PER_HOUR, LEISURE_MORALE_LIMIT,
        LEISURE_MORALE_SCALE_FATIGUE, LeisureOutcome, settlement_leisure_outcome,
    },
    strategic_time::MINUTES_PER_DAY,
};
use maud::{Markup, html};
use std::{collections::BTreeSet, fmt, str::FromStr};

use super::{
    decorative_game_icon, empty_state, game_icon, item_type_header, item_type_icon,
    population_description, quest_location_layout_with_session, settlement_layout_with_session,
    sidebar_section, stat_icon_path,
};
use crate::medical::MedicalPresentation;
use crate::routes::travel::{TravelDestination, TravelProvisionForecast};
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterCondition, CharacterEquip,
    CharacterLimbs, CharacterSkills, CharacterStats, CharacterStrategicCondition,
    CharacterTrainingSchedule, InventoryItem, InventoryQuantityTarget, ItemSlot, Party,
    PartyInventoryItem, PartyJourney, ScheduleAllocation, Settlement, SettlementAlias,
    SettlementDescription, SettlementDescriptionKind,
};

#[derive(Clone, Debug)]
pub struct LocationView {
    pub kind: LocationKind,
    pub id: String,
    pub name: String,
    pub religion_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ActivityPreviewRates {
    labor_gold_per_hour: f32,
    thievery_gold_per_hour: f32,
    thievery_virtue_per_hour: f32,
    raiding_gold_per_hour: f32,
    raiding_virtue_per_hour: f32,
    current_fatigue: f32,
}

impl ActivityPreviewRates {
    pub fn from_character(
        attributes: Option<&CharacterAttributes>,
        skills: Option<&CharacterSkills>,
        limbs: Option<&CharacterLimbs>,
        capability: Option<&CharacterCapability>,
        settlement: Option<&Settlement>,
        stats: Option<&CharacterStats>,
    ) -> Self {
        let current_fatigue = stats.map_or(0.0, |stats| stats.calories_used.max(0.0));
        let (Some(attributes), Some(skills), Some(limbs), Some(capability), Some(settlement)) =
            (attributes, skills, limbs, capability, settlement)
        else {
            return Self {
                current_fatigue,
                ..Self::default()
            };
        };
        let limb_health = [
            limbs.left_arm_health,
            limbs.right_arm_health,
            limbs.left_leg_health,
            limbs.right_leg_health,
        ];
        let strength = [
            attributes.left_arm_strength,
            attributes.right_arm_strength,
            attributes.left_leg_strength,
            attributes.right_leg_strength,
        ]
        .into_iter()
        .zip(limb_health)
        .map(|(value, health)| value * health.clamp(0.0, 1.0) * 0.25)
        .sum::<f32>();
        let agility = [
            attributes.left_arm_agility,
            attributes.right_arm_agility,
            attributes.left_leg_agility,
            attributes.right_leg_agility,
        ]
        .into_iter()
        .zip(limb_health)
        .map(|(value, health)| value * health.clamp(0.0, 1.0) * 0.25)
        .sum::<f32>();
        let usable_limbs = limb_health
            .into_iter()
            .map(|health| health.clamp(0.0, 1.0) * 0.25)
            .sum::<f32>();
        let precision = attributes.precision * usable_limbs;
        let stealth =
            (Skill::Stealth.training_rank(skills.stealth_hours) + agility + precision) * 0.5;
        let endurance = attributes.endurance * limbs.chest_health.clamp(0.0, 1.0);
        let population = settlement_population_scale(
            settlement.population_level,
            settlement.population_estimate,
        );
        let combat = capability
            .weapon_precision
            .max(capability.athletics)
            .max(capability.endurance);
        Self {
            labor_gold_per_hour: (strength.max(0.0) + endurance.max(0.0)) / 8.0,
            thievery_gold_per_hour: population.max(0.0) * (1.0 + stealth.max(0.0)) / 8.0,
            thievery_virtue_per_hour: -population.max(0.0) * 0.5 / (1.0 + stealth.max(0.0)),
            raiding_gold_per_hour: (2.0 + combat.max(0.0)) / 6.0,
            raiding_virtue_per_hour: -1.5,
            current_fatigue,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationKind {
    Settlement,
    Quest,
}

impl LocationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Settlement => "settlement",
            Self::Quest => "quest",
        }
    }
}

impl fmt::Display for LocationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LocationKind {
    type Err = ();
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "settlement" => Ok(Self::Settlement),
            "quest" => Ok(Self::Quest),
            _ => Err(()),
        }
    }
}

impl LocationView {
    pub fn base_path(&self) -> String {
        format!("/locations/{}/{}", self.kind, self.id)
    }

    fn render_layout(
        &self,
        title: &str,
        content: Markup,
        logged_in_as: Option<&str>,
        theme: &str,
    ) -> Markup {
        if self.kind == LocationKind::Settlement {
            settlement_layout_with_session(
                title,
                &self.name,
                &self.id,
                "",
                self.religion_id.as_deref(),
                content,
                logged_in_as,
                theme,
            )
        } else {
            quest_location_layout_with_session(
                title,
                &self.name,
                &self.id,
                "",
                content,
                logged_in_as,
                theme,
            )
        }
    }
}

/// The currently available merchant storefronts. They share trade mechanics,
/// but each storefront limits the stock shown on its left-hand side.
#[derive(Clone, Copy)]
pub enum MerchantShop {
    General,
    Weapons,
    Armor,
    Clothing,
}

pub struct RestSummary {
    pub minutes: u64,
    pub gold_spent: u32,
    pub gold_earned: u32,
    pub notoriety_gained: f32,
    pub healed: Vec<(String, f32)>,
    pub trained: Vec<(String, f32)>,
}

impl MerchantShop {
    pub fn service_id(self) -> &'static str {
        match self {
            Self::General => "merchants",
            Self::Weapons => "weapons",
            Self::Armor => "armor",
            Self::Clothing => "clothing",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "Market Square",
            Self::Weapons => "Weaponsmith",
            Self::Armor => "Armourer",
            Self::Clothing => "Tailor",
        }
    }

    fn stocks(self, kind: crate::spacetimedb::ItemKind) -> bool {
        match self {
            Self::General => kind != crate::spacetimedb::ItemKind::Currency,
            Self::Weapons => matches!(
                kind,
                crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Shield
            ),
            Self::Armor => kind == crate::spacetimedb::ItemKind::Armor,
            Self::Clothing => kind == crate::spacetimedb::ItemKind::Clothing,
        }
    }
}

/// Settlement information and the next destinations on the imported road and
/// ferry network.
pub fn settlement_overview_page(
    settlement: &Settlement,
    aliases: &[SettlementAlias],
    descriptions: &[SettlementDescription],
    active_character: Option<&Character>,
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let alias_labels = settlement_alias_labels(settlement, aliases);
    let historical_description = preferred_settlement_description(descriptions);
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Settlement", html! {
                div class="settlement-summary" {
                    dl class="location-stat-list" {
                        div { dt { "Population" } dd { (format_population(settlement)) } }
                        div { dt { "Size" } dd { (population_description(settlement.population_level)) } }
                        div { dt { "Coordinates" } dd { (format!("{}, {}", settlement.coord_x as i32, settlement.coord_y as i32)) } }
                        @if !alias_labels.is_empty() {
                            div { dt { "Also known as" } dd { (alias_labels.join(", ")) } }
                        }
                    }
                }
            }))
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None))
            (visual_stage("map", &settlement.name, "TODO: settlement image"))
            (settlement_chat_area(&settlement.name, active_character))
        }
        aside class="right-sidebar" {
            (sidebar_section("Description", html! {
                p { (settlement_description(settlement.population_level)) }
                @if let Some(description) = historical_description {
                    details class="settlement-historical-description" {
                        summary { (format!("Historical description — {}", language_label(description.language.as_deref()))) }
                        p { (description.body) }
                    }
                }
            }))
        }
    };
    settlement_layout_with_session(
        &settlement.name,
        &settlement.name,
        &settlement.id,
        "",
        Some(&settlement.religion_id),
        content,
        logged_in_as,
        theme,
    )
}

fn settlement_alias_labels(settlement: &Settlement, aliases: &[SettlementAlias]) -> Vec<String> {
    let canonical = settlement.name.to_lowercase();
    let mut labels = BTreeSet::new();
    for alias in aliases {
        let label = alias.prefix.as_ref().map_or_else(
            || alias.name.trim().to_owned(),
            |prefix| format!("{} {}", prefix.trim(), alias.name.trim()),
        );
        if !label.is_empty() && label.to_lowercase() != canonical {
            labels.insert(label);
        }
    }
    let total = labels.len();
    let mut labels: Vec<_> = labels.into_iter().take(8).collect();
    if total > labels.len() {
        labels.push(format!("and {} more", total - labels.len()));
    }
    labels
}

fn preferred_settlement_description(
    descriptions: &[SettlementDescription],
) -> Option<&SettlementDescription> {
    descriptions.iter().min_by_key(|description| {
        (
            description.language.as_deref() != Some("eng"),
            description.kind != SettlementDescriptionKind::Settlement,
            description.id.as_str(),
        )
    })
}

fn language_label(language: Option<&str>) -> &str {
    match language {
        Some("dan") => "Danish",
        Some("deu") => "German",
        Some("eng") => "English",
        Some("fin") => "Finnish",
        Some("nld") => "Dutch",
        Some(code) => code,
        None => "Unspecified language",
    }
}

pub fn settlement_map_page(
    settlement: &Settlement,
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    active_character: Option<&Character>,
    active_party: Option<&Party>,
    party_members: &[Character],
    can_travel: bool,
    provision_forecast: Option<&TravelProvisionForecast>,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let selected = selected_id.and_then(|id| destinations.iter().find(|entry| entry.id == id));
    let base_path = format!("/locations/settlement/{}/map", settlement.id);
    let content = html! {
        (map_destination_list(destinations, selected_id, &base_path))
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None))
            (visual_stage("map", &settlement.name, "TODO: settlement map"))
            (travel_planner_bar(selected, active_party.map_or(50, |party| party.camp_fatigue_percent)))
            (settlement_chat_area(&settlement.name, active_character))
        }
        (map_destination_detail(
            selected,
            can_travel,
            true,
            provision_forecast,
            active_party,
            active_party.is_some_and(|party| party.leader_id == active_character.map_or(0, |character| character.id)),
            &base_path,
        ))
    };
    settlement_layout_with_session(
        &format!("{} map", settlement.name),
        &settlement.name,
        &settlement.id,
        "map",
        Some(&settlement.religion_id),
        content,
        logged_in_as,
        theme,
    )
}

pub(crate) fn map_destination_list(
    destinations: &[TravelDestination],
    selected_id: Option<&str>,
    base_path: &str,
) -> Markup {
    html! {
        aside class="left-sidebar" {
            (sidebar_section("Destinations", html! {
                @if destinations.is_empty() {
                    (empty_state("No destinations are available from this location.", None, None))
                } @else {
                    nav class="location-destination-list" aria-label="Travel destinations" {
                        @for destination in destinations {
                            a href=(format!("{}?destination={}", base_path, destination.id))
                                class=(if selected_id == Some(destination.id.as_str()) { "list-item travel-destination-row active" } else { "list-item travel-destination-row" })
                                data-travel-name=(&destination.name)
                                data-travel-minutes=(destination.journey_minutes)
                                data-travel-camp-stops=(format_camp_stops(&destination.camp_stop_minutes))
                                data-travel-camp-forecasts=(format_camp_forecasts(destination))
                                data-travel-distance=(format_distance(destination.distance_m)) {
                                strong { (&destination.name) }
                                @if destination.quest_in_progress {
                                    span class="destination-quest-badge" title="Active quest destination"
                                        aria-label="Active quest destination" { "!" }
                                } @else if destination.active_quest_route {
                                    span class="destination-quest-badge" title="Next settlement toward active quest"
                                        aria-label="Next settlement toward active quest" { "!" }
                                } @else if destination.turn_in_ready {
                                    span class="destination-turn-in-badge" title="Quest ready to turn in here"
                                        aria-label="Quest ready to turn in here" { "!" }
                                }
                                span class="text-muted small-copy" { (format_distance(destination.distance_m)) }
                            }
                        }
                    }
                }
            }))
        }
    }
}

pub(crate) fn map_destination_detail(
    selected: Option<&TravelDestination>,
    can_travel: bool,
    provisioning_available: bool,
    provision_forecast: Option<&TravelProvisionForecast>,
    party: Option<&Party>,
    can_configure_travel: bool,
    map_path: &str,
) -> Markup {
    let camp_fatigue_percent = party.map_or(50, |party| party.camp_fatigue_percent);
    html! {
        aside class="right-sidebar" {
            @if party.is_some() && can_configure_travel {
            (sidebar_section("Travel configuration", html! {
                p class="text-muted small-copy" { "The party camps when its first member reaches this fatigue level." }
                form method="post" action=(format!("{map_path}/travel-configuration")) class="travel-configuration-form" data-travel-configuration {
                    label for="camp-fatigue-percent" { "Fatigue before camping" }
                    div class="travel-fatigue-control" {
                        input id="camp-fatigue-percent" type="range" name="fatigue_percent" min="10" max="100" step="5" value=(camp_fatigue_percent) aria-describedby="camp-fatigue-value" {}
                        output id="camp-fatigue-value" data-camp-fatigue-value { (format!("{camp_fatigue_percent}%")) }
                    }
                    p class="text-muted small-copy" data-travel-configuration-status { "Saved automatically when released." }
                }
            }))
            }
            @if let Some(destination) = selected {
                (sidebar_section(&destination.name, html! {
                    @if can_travel {
                        form method="post" action=(&destination.travel_action) data-travel-submit {
                            @if provisioning_available {
                                button type="submit" name="provisioning" value="provision" class="btn btn-primary btn-block" { "Provision and begin journey" }
                                button type="submit" name="provisioning" value="underprovisioned" class="btn btn-danger btn-block" { "Begin underprovisioned" }
                            } @else {
                                button type="submit" name="provisioning" value="underprovisioned" class="btn btn-primary btn-block" { "Begin journey" }
                            }
                        }
                        p class="travel-action-status" data-travel-action-status role="alert" hidden {}
                    }
                    p { (&destination.description) }
                    p class="text-muted small-copy" {
                        @if let Some(summary) = &destination.summary { (summary) " · " }
                        (format_distance(destination.distance_m))
                        " · " (format_journey_time(destination.journey_minutes))
                    }
                    @if can_travel && provisioning_available {
                        (travel_provision_forecast(provision_forecast))
                    }
                }))
            } @else {
                (sidebar_section("Destination", html! {
                    p class="text-muted small-copy" { "Select a destination to inspect it and plan travel." }
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
    let selected_minutes = selected.map_or(0, |destination| destination.journey_minutes);
    let selected_camp_stops = selected.map_or_else(String::new, |destination| {
        format_camp_stops(&destination.camp_stop_minutes)
    });
    let selected_camp_forecasts = selected.map_or_else(String::new, format_camp_forecasts);
    travel_planner_bar_for(
        selected_name,
        selected_minutes,
        &selected_camp_stops,
        &selected_camp_forecasts,
        camp_fatigue_percent,
        None,
    )
}

pub(crate) fn travel_planner_bar_for(
    destination_name: &str,
    journey_minutes: u64,
    camp_stop_minutes: &str,
    camp_forecasts: &str,
    camp_fatigue_percent: u8,
    journey: Option<&PartyJourney>,
) -> Markup {
    let journey_origin_name = journey.map_or("", |item| item.origin_name.as_str());
    let journey_destination_name = journey.map_or("", |item| item.destination_name.as_str());
    let journey_total_minutes = journey.map_or(0, |item| item.total_minutes);
    let journey_completed_minutes = journey.map_or(0, |item| item.completed_minutes);
    let journey_camp_stops = journey.map_or_else(String::new, |item| {
        format_camp_stops(&item.camp_stop_minutes)
    });
    let journey_forecast_stops = journey.map_or_else(String::new, |item| {
        format_camp_stops(&item.forecast_camp_stop_minutes)
    });
    html! {
        section class="travel-planner" data-travel-planner
            data-camp-fatigue-percent=(camp_fatigue_percent)
            data-selected-name=(destination_name)
            data-selected-minutes=(journey_minutes)
            data-selected-camp-stops=(camp_stop_minutes)
            data-selected-camp-forecasts=(camp_forecasts)
            data-journey-origin-name=(journey_origin_name)
            data-journey-destination-name=(journey_destination_name)
            data-journey-total-minutes=(journey_total_minutes)
            data-journey-completed-minutes=(journey_completed_minutes)
            data-journey-camp-stops=(journey_camp_stops)
            data-journey-forecast-stops=(journey_forecast_stops)
            aria-live="polite" hidden {
            div class="travel-planner-route" data-travel-planner-route {}
            p class="travel-planner-caption" data-travel-planner-caption {}
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

/// The transient strategic location between planned travel legs.
pub fn camp_page(
    party: &Party,
    journey: Option<&PartyJourney>,
    destination_name: &str,
    active_character: Option<&Character>,
    party_members: &[Character],
    default_rest_minutes: u64,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    let rest_hours = default_rest_minutes.div_ceil(60);
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Camp", html! {
                p { "The party has made camp between travel legs." }
                p class="text-muted small-copy" { "Destination: " (destination_name) }
                p class="text-muted small-copy" { (format_journey_time(party.camp_remaining_minutes)) " remaining" }
            }))
        }
        main class="center-content settlement-main settlement-overview" {
            (party_portrait_overlay(party_members, active_character, "/camp", None))
            (visual_stage("camp", "Camp", "The party is resting beside the road."))
            (travel_planner_bar_for(destination_name, party.camp_remaining_minutes, "", "", party.camp_fatigue_percent, journey))
            (settlement_chat_area("Camp", active_character))
        }
        aside class="right-sidebar" {
            (sidebar_section("Journey", html! {
                p class="text-muted small-copy" { "Break camp to travel the next planned leg." }
                form action="/camp/continue" method="post" data-travel-submit {
                    button type="submit" class="btn btn-primary btn-small btn-block" { "Continue travel" }
                }
                p class="travel-action-status" data-travel-action-status role="alert" hidden {}
            }))
            section class="rest-service-menu camp-rest-menu" aria-label="Camp rest" {
                div class="rest-service-heading" { strong { "Rest at camp" } }
                p class="rest-service-copy" { "The whole party rests. Camping is free and does not replenish provisions." }
                form action="/camp/rest" method="post" {
                    (rest_duration_control("camp-rest", rest_hours, "hours", "Rest the party"))
                    button type="submit" class="btn btn-primary btn-small btn-block" data-rest-submit
                        disabled[rest_hours == 0] { "Rest party" }
                }
            }
        }
    };
    quest_location_layout_with_session(
        "Camp",
        "Camp",
        &party.id,
        "camp",
        content,
        logged_in_as,
        theme,
    )
}

fn rest_duration_control(_id_prefix: &str, value: u64, unit: &str, label: &str) -> Markup {
    let hours_active = unit == "hours";
    html! {
        div class="rest-duration-control" data-rest-duration {
            div class="rest-duration-units" role="radiogroup" aria-label=(label) {
                label class=(if hours_active { "rest-duration-unit active" } else { "rest-duration-unit" }) {
                    input type="radio" name="unit" value="hours" checked[hours_active] {}
                    "Hours"
                }
                label class=(if !hours_active { "rest-duration-unit active" } else { "rest-duration-unit" }) {
                    input type="radio" name="unit" value="days" checked[!hours_active] {}
                    "Days"
                }
            }
            div class="rest-days-control" {
                button type="button" class="rest-days-step rest-days-decrease" aria-label="Decrease rest duration"
                    onclick="const input=this.parentElement.querySelector('input'); input.value=Math.max(0, Number(input.value || 0)-1); input.dispatchEvent(new Event('input', {bubbles:true}));" { "−" }
                input type="number" name="duration" value=(value) min="0" max="365" aria-label=(label)
                    oninput="this.form.querySelector('[type=submit]').disabled=Number(this.value || 0) <= 0;";
                span class="rest-days-unit" data-rest-unit-label { (unit) }
                button type="button" class="rest-days-step rest-days-increase" aria-label="Increase rest duration"
                    onclick="const input=this.parentElement.querySelector('input'); input.value=Math.min(Number(input.max || 365), Number(input.value || 0)+1); input.dispatchEvent(new Event('input', {bubbles:true}));" { "+" }
            }
        }
    }
}

fn travel_provision_forecast(forecast: Option<&TravelProvisionForecast>) -> Markup {
    html! {
        div class="travel-provision-forecast" {
            strong { "Provision forecast" }
            @if let Some(forecast) = forecast {
                @for traveler in &forecast.travelers {
                    p class="text-muted small-copy" {
                        (&traveler.name) ": "
                        (traveler.rations_to_buy) " ration" @if traveler.rations_to_buy != 1 { "s" }
                        " · " (traveler.waterskins_to_buy) " waterskin" @if traveler.waterskins_to_buy != 1 { "s" }
                        " · " (traveler.cost) " gold"
                    }
                }
                p class="text-muted small-copy" {
                    "Party total: " (forecast.total_cost) " gold · includes 30% reserve"
                }
            } @else {
                p class="text-muted small-copy" { "Provision costs are temporarily unavailable." }
            }
        }
    }
}

pub(crate) fn settlement_description(population_level: i32) -> &'static str {
    match population_level {
        i32::MIN..=1 => "A quiet cluster of farmsteads and cottages.",
        2 => "A quaint hamlet gathered around a well-worn road.",
        3 => "A modest village serving the surrounding countryside.",
        4 => "A busy market town with a steady flow of travelers.",
        5 => "A prosperous town enclosed by crowded streets.",
        _ => "A large and bustling city whose streets rarely fall silent.",
    }
}

fn format_distance(distance_m: u64) -> String {
    format!("{:.1} km", distance_m as f64 / 1_000.0)
}

fn format_population(settlement: &Settlement) -> String {
    match settlement.population_estimate {
        0 => population_description(settlement.population_level).to_string(),
        population => format!("approximately {}", format_number(population)),
    }
}

fn format_number(value: u32) -> String {
    let digits = value.to_string();
    let first_group = match digits.len() % 3 {
        0 => 3,
        remainder => remainder,
    };
    let mut formatted = digits[..first_group].to_string();
    for group in digits[first_group..].as_bytes().chunks(3) {
        formatted.push(',');
        formatted.push_str(std::str::from_utf8(group).expect("population digits are valid UTF-8"));
    }
    formatted
}

fn format_journey_time(minutes: u64) -> String {
    let hours = minutes / 60;
    let minutes = minutes % 60;
    if hours == 0 {
        format!("{minutes} min")
    } else if minutes == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {minutes} min")
    }
}

/// Market interface. Inventory and prices are intentionally UI-only placeholders
/// until settlement-owned inventory and trade reducers exist.
pub fn merchants_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "merchants",
        "Market Square",
        "Market Steward",
        "Merchant stock and prices will appear here once the trade backend is available.",
        active_character,
        inventory,
        &[],
        party_members,
        logged_in_as,
        theme,
        None,
        None,
    )
}

/// Inn interface placeholder.
pub fn inn_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    limbs: Option<&CharacterLimbs>,
    stats: Option<&CharacterStats>,
    condition: Option<&CharacterCondition>,
    field_repair_minutes: u64,
    smith_wait_minutes: u64,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "inn",
        "The Inn",
        "Innkeeper",
        "Rest duration, recovery, training, and strategic time advancement are not connected yet.",
        active_character,
        inventory,
        items,
        party_members,
        logged_in_as,
        theme,
        rest_default_minutes(
            limbs,
            stats,
            condition,
            field_repair_minutes,
            smith_wait_minutes,
        ),
        None,
    )
}

/// Church placeholder.
pub fn religion_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    limbs: Option<&CharacterLimbs>,
    stats: Option<&CharacterStats>,
    condition: Option<&CharacterCondition>,
    field_repair_minutes: u64,
    smith_wait_minutes: u64,
    logged_in_as: Option<&str>,
    theme: &str,
) -> Markup {
    service_page(
        settlement,
        "religion",
        "Church",
        "Priest",
        "Faith, donations, and divine services require the religion and reputation systems.",
        active_character,
        inventory,
        items,
        party_members,
        logged_in_as,
        theme,
        rest_default_minutes(
            limbs,
            stats,
            condition,
            field_repair_minutes,
            smith_wait_minutes,
        ),
        None,
    )
}

/// Party inventory comparison.
pub fn party_inventory_page(
    location: &LocationView,
    selected: &Character,
    selected_inventory: &[InventoryItem],
    active_character: &Character,
    active_inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    selected_equip: Option<&CharacterEquip>,
    active_equip: Option<&CharacterEquip>,
    selected_targets: &[InventoryQuantityTarget],
    active_targets: &[InventoryQuantityTarget],
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (party_trade_inventory_rail(selected, selected_inventory, items, active_character.id, "right", selected_equip, active_targets))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(selected.id)))
            (visual_stage("npc", &selected.name, &format!("TODO: {} portrait", selected.name.to_lowercase())))
            (player_chat_area(selected, active_character))
            form id="party-offer" class="party-offer" action=(format!("{}/party/{}/inventory/offer", location.base_path(), selected.id)) method="post" hidden {
                button type="button" class="party-offer-cancel" data-cancel-trade="party" { "Cancel" }
                button type="submit" disabled { "Offer" }
            }
        }
        aside class="right-sidebar" {
            (party_trade_inventory_rail(active_character, active_inventory, items, selected.id, "left", active_equip, selected_targets))
        }
    };
    location.render_layout("Party", content, Some(&active_character.name), theme)
}

/// The active character's inventory with a staged discard list.
pub fn party_discard_page(
    location: &LocationView,
    active_character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Discard", html! {
                p class="text-muted small-copy" data-discard-empty { "Stage carried items here before discarding them." }
                table class="trade-inventory-table" data-discard-table hidden {
                    (trade_inventory_table_header(false, None))
                    tbody data-discard-list {}
                }
            }))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(active_character.id)))
            (visual_stage("npc", &active_character.name, &format!("TODO: {} portrait", active_character.name.to_lowercase())))
            (settlement_chat_area(&active_character.name, Some(active_character)))
            form id="inventory-discard" class="party-offer"
                action=(format!("{}/party/{}/inventory/discard", location.base_path(), active_character.id))
                method="post" hidden {
                button type="button" class="party-offer-cancel" data-cancel-trade="discard" { "Cancel" }
                button type="submit" disabled { "Discard" }
            }
        }
        aside class="right-sidebar" {
            (discard_inventory_rail(active_character, inventory, items, equip))
        }
    };
    location.render_layout("Inventory", content, Some(&active_character.name), theme)
}

/// Active character's combined strategic view.
pub fn party_personal_page(
    location: &LocationView,
    active_character: &Character,
    party_members: &[Character],
    capability: Option<&CharacterCapability>,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    condition: Option<&CharacterStrategicCondition>,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    religion_id: Option<&str>,
    schedule: Option<&CharacterTrainingSchedule>,
    activity_preview: ActivityPreviewRates,
    religious_demand: Option<&crate::spacetimedb::ReligiousDemand>,
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (character_summary_rail(capability))
            (party_attributes_rail("Your attributes", attributes, limbs))
            @let schedule_action = format!("{}/party/{}/schedule", location.base_path(), active_character.id);
            (party_skills_rail("Your skills", skills, limbs, schedule, Some(&schedule_action), Some(activity_preview)))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(active_character.id)))
            (visual_stage("npc", &active_character.name, &format!("TODO: {} portrait", active_character.name.to_lowercase())))
            (settlement_chat_area(&active_character.name, Some(active_character)))
        }
        aside class="right-sidebar" {
            (strategic_condition_rail(condition, morale_sources))
            (medical_rail(medical, &location.base_path(), active_character.id, active_character.id, true))
            @if let Some(demand) = religious_demand {
                (religious_demand_rail(demand, &location.base_path(), active_character.id))
            }
            (character_bio_rail(active_character, religion_id, notoriety, personality, true, &location.base_path()))
        }
    };
    location.render_layout("Party", content, Some(&active_character.name), theme)
}

fn religious_demand_rail(
    demand: &crate::spacetimedb::ReligiousDemand,
    location_path: &str,
    character_id: u64,
) -> Markup {
    let action = format!(
        "{location_path}/party/{character_id}/religious-demand/{}",
        demand.id
    );
    html! {
        (sidebar_section("Conviction demands", html! {
            article class="religious-demand" {
                h3 { (&demand.title) }
                p { (&demand.description) }
                p class="text-muted small-copy" {
                    "Observe and bear the practical cost, or decline. Party Charisma automatically reduces the morale cost of neglect and can remove it entirely."
                }
                form method="post" action=(action) class="religious-demand-actions" {
                    button type="submit" name="choice" value="observe" class="btn btn-primary" { "Observe" }
                    button type="submit" name="choice" value="refuse" class="btn btn-danger" { "Do not observe" }
                }
            }
        }))
    }
}

/// Selected party member stats and biography.
pub fn party_stats_page(
    location: &LocationView,
    selected: &Character,
    active_character: &Character,
    party_members: &[Character],
    capability: Option<&CharacterCapability>,
    selected_attributes: Option<&CharacterAttributes>,
    selected_skills: Option<&CharacterSkills>,
    selected_limbs: Option<&CharacterLimbs>,
    condition: Option<&CharacterStrategicCondition>,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    religion_id: Option<&str>,
    active_party: Option<&Party>,
    selected_party: Option<&Party>,
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    theme: &str,
) -> Markup {
    let selected_attributes_title = format!("{}'s attributes", selected.name);
    let selected_skills_title = format!("{}'s skills", selected.name);
    let content = html! {
        aside class="left-sidebar" {
            (character_summary_rail(capability))
            (party_attributes_rail(&selected_attributes_title, selected_attributes, selected_limbs))
            (party_skills_rail(&selected_skills_title, selected_skills, selected_limbs, None, None, None))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(selected.id)))
            (visual_stage("npc", &selected.name, &format!("TODO: {} portrait", selected.name.to_lowercase())))
            (player_chat_area(selected, active_character))
        }
        aside class="right-sidebar" {
            (strategic_condition_rail(condition, morale_sources))
            (medical_rail(medical, &location.base_path(), active_character.id, selected.id, true))
            (character_bio_rail(
                selected,
                religion_id,
                notoriety,
                personality,
                selected.id == active_character.id,
                &location.base_path(),
            ))
            @if selected.id != active_character.id {
                @if active_character.party_id == selected.party_id {
                    @if active_party.is_some_and(|party| party.leader_id == selected.id) {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{}/party/{}/remove", location.base_path(), active_character.id)) {
                                button type="submit" class="btn btn-danger btn-block" { "Leave party" }
                            }
                        }))
                    } @else {
                        (sidebar_section("Party", html! {
                            form method="post" action=(format!("{}/party/{}/remove", location.base_path(), selected.id)) {
                                button type="submit" class="btn btn-danger btn-block" {
                                    @if active_party.is_some_and(|party| party.leader_id == active_character.id) { "Kick from party" }
                                    @else { "Request kick" }
                                }
                            }
                        }))
                    }
                } @else if let Some(party) = selected_party {
                    (sidebar_section("Party", html! {
                        p { (&party.name) }
                        form method="post" action=(format!("/parties/{}/join-general", party.id)) {
                            button type="submit" class="btn btn-primary btn-block" { "Request to join party" }
                        }
                    }))
                }
            }
        }
    };
    location.render_layout("Party stats", content, Some(&active_character.name), theme)
}

fn service_page(
    settlement: &Settlement,
    service_id: &str,
    title: &str,
    npc_name: &str,
    todo: &str,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
    rest_default_minutes: Option<u64>,
    rest_summary: Option<&RestSummary>,
) -> Markup {
    let trade_offers: Option<(&str, &[&str])> = match service_id {
        "merchants" => Some((
            "Merchant stock",
            &["Weapon offer", "Armour offer", "Provision offer"],
        )),
        "weapons" => Some((
            "Weapons",
            &["Weapon offer", "Shield offer", "Ammunition offer"],
        )),
        "armor" => Some((
            "Armour",
            &["Head protection", "Torso protection", "Limb protection"],
        )),
        "clothing" => Some((
            "Clothing",
            &["Travel attire", "Cold-weather clothing", "Fine clothing"],
        )),
        "inn" => Some((
            "Inn supplies",
            &["Rations", "Water", "Supplies", "Bed for the night"],
        )),
        _ => None,
    };
    let content = html! {
        aside class=(if service_id == "inn" || service_id == "religion" { "left-sidebar service-left-sidebar" } else { "left-sidebar" }) {
            @if service_id == "inn" {
                div class="service-left-stack" {
                    div class="service-inventory-area" { (merchant_offers_rail("Inn supplies", &["Rations", "Water", "Supplies", "Bed for the night"])) }
                    (rest_service_menu("Inn", &settlement.id, "inn", rest_default_minutes, rest_summary))
                }
            } @else if service_id == "religion" {
                div class="service-left-stack" {
                    div class="service-inventory-area" {
                        (sidebar_section("Church services", html! {
                            p { "Faith: " strong { (religion_name(Some(&settlement.religion_id))) } }
                            @if active_character.is_some() {
                                p class="small-copy" { "Speak with the priest below to profess this church's faith. Renunciation is available only from your biography." }
                            }
                            p class="text-muted small-copy" { "Shared conviction strengthens allied Charisma. Conflicting conviction turns that influence into a morale penalty." }
                        }))
                    }
                    (rest_service_menu("Temple", &settlement.id, "temple", rest_default_minutes, rest_summary))
                }
            } @else if let Some((stock_title, offers)) = trade_offers {
                (merchant_offers_rail(stock_title, offers))
            } @else {
                (sidebar_section("Settlement offerings", html! {
                    div class="service-placeholder-list" {
                        span { "Inventory / offers" }
                        span class="badge badge-warning" { "TODO" }
                    }
                    p class="text-muted small-copy" { (todo) }
                }))
            }
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None))
            (visual_stage("npc", npc_name, &format!("TODO: {} portrait", npc_name.to_lowercase())))
            (settlement_service_chat_area(
                title,
                active_character,
                &settlement.id,
                service_id,
            ))
        }
        aside class="right-sidebar" {
            @if trade_offers.is_some() {
                (inventory_rail(
                    active_character,
                    inventory,
                    items,
                    Some(("Sell", "TODO: selling requires merchant pricing and trade reducers")),
                    matches!(service_id, "weapons" | "armor" | "clothing"),
                ))
            } @else if service_id == "smith" {
                (inventory_rail(
                    active_character,
                    inventory,
                    items,
                    Some(("Repair", "TODO: repairs require durability, pricing, and smithing reducers")),
                    true,
                ))
            } @else if service_id == "religion" {
                (inventory_rail(active_character, inventory, items, None, false))
            } @else {
                (sidebar_section("Service", html! {
                    p class="text-muted small-copy" { (todo) }
                }))
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        service_id,
        Some(&settlement.religion_id),
        content,
        logged_in_as,
        theme,
    )
}

fn party_trade_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    recipient_id: u64,
    direction: &str,
    equip: Option<&CharacterEquip>,
    recipient_targets: &[InventoryQuantityTarget],
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                (trade_inventory_table(true, None, html! {
                    @for item in inventory {
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let target = target_quantity(recipient_targets, &item.item_id);
                            tr class=(if direction == "left" { "trade-inventory-row trade-row-player" } else { "trade-inventory-row trade-row-merchant" }) data-item-key=(&item.item_id) {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_quality(&item.item_id, definition))
                                    @if !is_equipped { span class="inventory-row-actions" { button type="button" class=(format!("trade-transfer trade-transfer-{direction} party-draft-transfer")) data-dynamic-transfer data-default-transfer-mode="one" data-from=(character.id) data-to=(recipient_id) data-item=(item.id) data-key=(&item.item_id) data-count=(item.qty) data-target=(target) data-transfer-mode="one" data-label-one=(format!("Transfer one {}", item.item_id)) data-label-target=(format!("Transfer {} to target", item.item_id)) data-label-all=(format!("Transfer all {}", item.item_id)) aria-label=(format!("Transfer one {}", item.item_id)) title=(format!("Transfer one {}", item.item_id)) { (transfer_glyph(1)) } } }
                                }
                                td class="inventory-count" { (item.qty) }
                                td class="inventory-equipped" { (equipment_checkbox(item, definition, is_equipped)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                    }
                }))
                (inventory_footer_controls(if direction == "left" { "party-left" } else { "party-right" }, "Transfer to targets", "Transfer everything"))
            }
        }))
    }
}

fn discard_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    equip: Option<&CharacterEquip>,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                (trade_inventory_table(true, None, html! {
                    @for item in inventory {
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-discard-source=(item.id) data-item-key=(&item.item_id) {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" {
                                (item_name_with_quality(&item.item_id, definition))
                                @if !is_equipped {
                                    button type="button" class="trade-transfer trade-transfer-left"
                                        data-discard-item=(item.id) data-count=(item.qty)
                                        data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one"
                                        data-label-one=(format!("Discard one {}", item.item_id))
                                        data-label-target=(format!("Discard {} down to target", item.item_id))
                                        data-label-all=(format!("Discard all {}", item.item_id))
                                        aria-label=(format!("Discard {}", item.item_id))
                                        title=(format!("Discard one {}", item.item_id)) { (transfer_glyph(1)) }
                                }
                            }
                            td class="inventory-count" { (item.qty) }
                            td class="inventory-equipped" { (equipment_checkbox(item, definition, is_equipped)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
            }
        }))
    }
}

pub fn live_merchant_shop_page(
    settlement: &Settlement,
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    pooled: &[PartyInventoryItem],
    theme: &str,
    shop: MerchantShop,
    conditions: &[crate::spacetimedb::ItemCondition],
    smith: Option<&crate::spacetimedb::SettlementSmith>,
    repair_orders: &[crate::spacetimedb::RepairOrder],
    now_minutes: u64,
) -> Markup {
    let title = shop.title();
    let service_id = shop.service_id();
    let smith_skill = smith
        .map(|smith| {
            if matches!(shop, MerchantShop::Armor) {
                smith.armourer_skill
            } else {
                smith.weaponsmith_skill
            }
        })
        .unwrap_or(0);
    let content = html! {
        aside class="left-sidebar smith-wares-column" { (sidebar_section("Merchant stock", html! {
            div class="smith-wares-scroll" {
            (trade_inventory_table(false, None, html! {
                @for item in items.iter().filter(|item| shop.stocks(item.kind)) {
                    @let buy_price = (item.base_value.unwrap_or(1) as f32 * 1.375).ceil() as u32;
                    @let sell_price = (item.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32;
                    @let target = target_quantity(personal_targets, &item.id);
                    tr class="trade-inventory-row trade-row-merchant" data-merchant-item=(&item.id) data-merchant-sell-price=(sell_price) { td class="inventory-item-type" { (item_type_icon(&item.id)) } td class="inventory-item-name" { (item_name_with_quality(&item.id, Some(item))) (merchant_buy_controls(&item.id, buy_price, target, 999)) } td class="inventory-count" { "999" } td class="inventory-weight" { (weight_display(item.weight)) } td class="inventory-gold" { (buy_price) } }
                }
            }))
            (inventory_footer_controls("buy", "Buy to targets", "Buy everything"))
            }
        }))
        @if matches!(shop, MerchantShop::Weapons | MerchantShop::Armor) {
            (repair_custody_panel(settlement, shop, repair_orders, conditions, items, now_minutes, smith_skill))
        }
        }
        main class="center-content settlement-main" { (party_portrait_overlay(party_members, Some(character), &format!("/locations/settlement/{}", settlement.id), None)) (visual_stage("npc", title, &format!("TODO: {} portrait", title.to_lowercase()))) (settlement_service_chat_area(title, Some(character), &settlement.id, service_id)) form # "merchant-offer" class="party-offer" action=(format!("/settlements/{}/merchants/offer", settlement.id)) method="post" hidden { input type="hidden" name="return_to" value=(service_id); input type="hidden" name="inventory_scope" value="player"; button type="button" class="party-offer-cancel" data-cancel-trade="merchant" { "Cancel" } button type="submit" disabled { "Offer" } } }
        aside class="right-sidebar inventory-owner-panel" data-inventory-tabs {
            nav class="inventory-owner-tabs" aria-label="Trading inventory" {
                button type="button" class="inventory-owner-tab active" data-inventory-tab="player" { "Player" }
                button type="button" class="inventory-owner-tab" data-inventory-tab="party" { "Party" }
            }
            div data-inventory-pane="player" {
            div class="sidebar-section" {
                (trade_inventory_table(true, matches!(shop, MerchantShop::Weapons | MerchantShop::Armor).then(|| repair_all_header(settlement, service_id)), html! {
                    @for item in inventory {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let sell_price = definition.map_or(0, |definition| (definition.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32);
                        @let target = target_quantity(personal_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-merchant-equipped=(is_equipped) data-inventory-quantity=(item.qty) data-target=(target) {
                        @let condition = conditions.iter().find(|condition| condition.inventory_item_id == item.id);
                        @let repair_skill = smith_skill;
                        @let durable_item = definition.is_some_and(|definition| matches!(definition.kind, crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Armor | crate::spacetimedb::ItemKind::Shield));
                        @let service_matches = definition.is_some_and(|definition| if matches!(shop, MerchantShop::Armor) { definition.kind == crate::spacetimedb::ItemKind::Armor } else { matches!(definition.kind, crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Shield) });
                        @let can_sell = !is_currency && !is_equipped;
                        td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                        td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) @if can_sell || service_matches { (merchant_sell_repair_controls(item.id, &item.item_id, sell_price, item.qty, target, can_sell, service_matches.then(|| repair_submit_control(settlement, service_id, item.id, condition, repair_skill)))) } }
                        td class="inventory-count" { (quantity_target_control(item.qty, target, &item.item_id, false)) } td class="inventory-equipped" { (equipment_checkbox(item, definition, is_equipped)) } td class="inventory-durability" { @if durable_item { (condition_bar(condition, service_matches.then_some(repair_skill))) } @else { "—" } } td class="inventory-weight" { (item_weight(definition)) } td class="inventory-gold" { (sell_price) }
                    }}
                    @for target in personal_targets.iter().filter(|target| target.quantity > 0 && !inventory.iter().any(|item| item.item_id == target.item_id)) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-type" { (item_type_icon(&target.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&target.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, false)) }
                            td class="inventory-equipped" { input type="checkbox" disabled; }
                            td class="inventory-durability" { "—" }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                (inventory_footer_controls("sell", "Sell surplus", "Sell everything"))
            }
            }
            div data-inventory-pane="party" hidden {
            div class="sidebar-section" {
                (trade_inventory_table(false, None, html! {
                    @for item in pooled {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let sell_price = definition.map_or(0, |definition| (definition.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32);
                        @let target = target_quantity(party_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-party-inventory-id=(item.id) data-inventory-quantity=(item.quantity) data-target=(target) {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) @if !is_currency { (merchant_sell_controls(item.id, &item.item_id, sell_price, item.quantity, target)) } }
                            td class="inventory-count" { (quantity_target_control(item.quantity, target, &item.item_id, true)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (sell_price) }
                        }
                    }
                    @for target in party_targets.iter().filter(|target| target.quantity > 0 && !pooled.iter().any(|item| item.item_id == target.item_id)) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-type" { (item_type_icon(&target.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&target.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, true)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                (inventory_footer_controls("sell", "Sell surplus", "Sell everything"))
            }
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        service_id,
        Some(&settlement.religion_id),
        content,
        Some(&character.name),
        theme,
    )
}

/// Two-sided transfer view for the equally owned party chest.
pub fn party_pool_page(
    location: &LocationView,
    character: &Character,
    inventory: &[InventoryItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    theme: &str,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Party inventory", html! {
                div class="party-stake-summary" {
                    span { "Your available stake" }
                    strong { (stake) " gold" }
                }
                p class="small-copy text-muted" { "Withdrawals use your stake. Personal gold automatically covers an indivisible item's shortfall." }
                (trade_inventory_table(false, None, html! {
                    @for item in pooled {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let value = definition.and_then(|definition| definition.base_value).unwrap_or(0) as u64;
                        @let target = target_quantity(personal_targets, &item.item_id);
                        @let current = inventory.iter().find(|personal| personal.item_id == item.item_id).map_or(0, |personal| personal.qty);
                        tr class="trade-inventory-row" {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" {
                                (item_name_with_quality(&item.item_id, definition))
                                span class="inventory-row-actions" { button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="withdraw" data-transfer-mode="one" data-count=(item.quantity) data-current=(current) data-target=(target) data-label-one=(format!("Withdraw one {}", item.item_id)) data-label-target=(format!("Withdraw {} to target", item.item_id)) data-label-all=(format!("Withdraw all {}", item.item_id)) title=(if value > stake { format!("Withdraw one {}; {} personal gold required", item.item_id, value - stake) } else { format!("Withdraw one {} using your stake", item.item_id) }) aria-label=(format!("Withdraw one {}", item.item_id)) { (transfer_glyph(1)) } }
                            }
                            td class="inventory-count" { (quantity_target_control(item.quantity, target_quantity(party_targets, &item.item_id), &item.item_id, true)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                (inventory_footer_controls("withdraw", "Withdraw to personal targets", "Withdraw everything"))
            }))
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, Some(character), &location.base_path(), None))
            (visual_stage("npc", "Party chest", "Shared party inventory chest"))
            (settlement_chat_area("Party inventory", Some(character)))
        }
        aside class="right-sidebar" {
            (sidebar_section(&format!("{}'s inventory", character.name), html! {
                p class="small-copy text-muted" { "Add items at their objective gold value." }
                (trade_inventory_table(true, None, html! {
                    @for item in inventory {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let target = target_quantity(party_targets, &item.item_id);
                        @let current = pooled.iter().find(|pooled| pooled.item_id == item.item_id).map_or(0, |pooled| pooled.quantity);
                        tr class="trade-inventory-row" {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" {
                                (item_name_with_quality(&item.item_id, definition))
                                @if !equipped {
                                    span class="inventory-row-actions" { button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="deposit" data-transfer-mode="one" data-count=(item.qty) data-current=(current) data-target=(target) data-label-one=(format!("Deposit one {}", item.item_id)) data-label-target=(format!("Deposit {} to target", item.item_id)) data-label-all=(format!("Deposit all {}", item.item_id)) aria-label=(format!("Deposit one {}", item.item_id)) title=(format!("Deposit one {}", item.item_id)) { (transfer_glyph(1)) } }
                                }
                            }
                            td class="inventory-count" { (quantity_target_control(item.qty, target_quantity(personal_targets, &item.item_id), &item.item_id, false)) }
                            td class="inventory-equipped" { (equipment_checkbox(item, definition, equipped)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                (inventory_footer_controls("deposit", "Deposit to party targets", "Deposit everything"))
            }))
        }
        form method="post" action=(format!("{}/party-inventory/deposit", location.base_path())) id="pool-transfer-offer" class="party-offer" hidden { button type="button" data-cancel-pool class="party-offer-cancel" { "Cancel" } button type="submit" disabled { "Offer" } }
    };
    location.render_layout("Party inventory", content, Some(&character.name), theme)
}

fn item_weight(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.map_or_else(|| "—".to_owned(), |item| weight_display(item.weight))
}

fn equipment_checkbox(
    inventory: &InventoryItem,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    equipped: bool,
) -> Markup {
    let equippable = definition.is_some_and(|definition| definition.slot != ItemSlot::None);
    let label = if equipped {
        format!("Unequip {}", inventory.item_id)
    } else {
        format!("Equip {}", inventory.item_id)
    };
    html! {
        input type="checkbox"
            checked[equipped]
            disabled[!equippable]
            data-equipment-toggle
            data-inventory-item-id=(inventory.id)
            aria-label=(label)
            title=(if equippable { "Equip or unequip this item" } else { "This item cannot be equipped" });
    }
}

fn item_value(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.and_then(|item| item.base_value)
        .map_or_else(|| "—".to_owned(), |value| value.to_string())
}

pub(super) fn item_name_with_quality(
    item_id: &str,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
) -> Markup {
    let quality = definition
        .filter(|item| {
            matches!(
                item.kind,
                crate::spacetimedb::ItemKind::Weapon
                    | crate::spacetimedb::ItemKind::Armor
                    | crate::spacetimedb::ItemKind::Shield
            )
        })
        .map(|item| item.quality.clamp(1, 5));
    let label = quality.map(|quality| match quality {
        1 => "Quality 1",
        2 => "Quality 2",
        3 => "Quality 3 — munition grade",
        4 => "Quality 4 — knightly commission",
        5 => "Quality 5 — royal or heroic commission",
        _ => unreachable!(),
    });
    html! {
        span class=(quality.map_or_else(|| "inventory-item-label".to_string(), |quality| format!("inventory-item-label item-quality-{quality}"))) title=[label] {
            (item_id)
        }
    }
}

fn weight_display(weight: f32) -> String {
    let display = format!("{weight:.2}");
    display
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn trade_inventory_table(
    show_equipped: bool,
    condition_header: Option<Markup>,
    rows: Markup,
) -> Markup {
    let show_condition = condition_header.is_some();
    html! {
        table class=(if show_condition { "trade-inventory-table smith-player-inventory-table" } else { "trade-inventory-table" }) {
            colgroup {
                col class="inventory-column-type";
                col class="inventory-column-item";
                col class="inventory-column-count";
                @if show_equipped { col class="inventory-column-equipped"; }
                @if show_condition { col class="inventory-column-durability"; }
                col class="inventory-column-weight";
                col class="inventory-column-gold";
            }
            (trade_inventory_table_header(show_equipped, condition_header))
            tbody { (rows) }
        }
    }
}

fn target_quantity(targets: &[InventoryQuantityTarget], item_id: &str) -> u32 {
    targets
        .iter()
        .find(|target| target.item_id == item_id)
        .map_or(0, |target| target.quantity)
}

fn quantity_target_control(quantity: u32, target: u32, item_id: &str, party_scope: bool) -> Markup {
    html! {
        span class="inventory-target-control" data-target-control data-item-id=(item_id) data-party-scope=(party_scope) title=(format!("Carrying {quantity}; target {target}")) {
            span class="inventory-target-prefix" { (quantity) "/" }
            span class="inventory-target-denominator" {
                button type="button" class="inventory-target-step inventory-target-up" data-target-step="1" aria-label=(format!("Increase {} target", item_id)) { "⌃" }
                span class="inventory-target-value" data-target-value { (target) }
                button type="button" class="inventory-target-step inventory-target-down" data-target-step="-1" aria-label=(format!("Decrease {} target", item_id)) hidden[target == 0] { "⌄" }
            }
        }
    }
}

pub(crate) fn transfer_glyph(count: usize) -> Markup {
    html! { span class=(format!("inventory-transfer-glyph arrows-{count}")) aria-hidden="true" { @for _ in 0..count { i {} } } }
}

fn merchant_buy_controls(item_id: &str, price: u32, target: u32, available: u32) -> Markup {
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-buy=(item_id) data-merchant-buy-price=(price) data-transfer-mode="one" data-target=(target) data-count=(available) data-label-one=(format!("Buy one {item_id}")) data-label-target=(format!("Buy {item_id} to target")) data-label-all=(format!("Buy all {item_id}")) aria-label=(format!("Buy one {item_id}")) title=(format!("Buy one {item_id}")) { (transfer_glyph(1)) }
    } }
}

fn merchant_sell_controls(
    id: u64,
    item_id: &str,
    price: u32,
    quantity: u32,
    target: u32,
) -> Markup {
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-sell=(id) data-item-name=(item_id) data-merchant-sell-price=(price) data-transfer-mode="one" data-count=(quantity) data-target=(target) data-label-one=(format!("Sell one {item_id}")) data-label-target=(format!("Sell surplus {item_id}")) data-label-all=(format!("Sell all {item_id}")) aria-label=(format!("Sell one {item_id}")) title=(format!("Sell one {item_id}")) { (transfer_glyph(1)) }
    } }
}

fn merchant_sell_repair_controls(
    id: u64,
    item_id: &str,
    price: u32,
    quantity: u32,
    target: u32,
    can_sell: bool,
    repair: Option<Markup>,
) -> Markup {
    html! { div class="inventory-row-actions smith-player-actions" {
        @if can_sell {
            button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-sell=(id) data-item-name=(item_id) data-merchant-sell-price=(price) data-transfer-mode="one" data-count=(quantity) data-target=(target) data-label-one=(format!("Sell one {item_id}")) data-label-target=(format!("Sell surplus {item_id}")) data-label-all=(format!("Sell all {item_id}")) aria-label=(format!("Sell one {item_id}")) title=(format!("Sell one {item_id}")) { (transfer_glyph(1)) }
        }
        @if let Some(repair) = repair { (repair) }
    } }
}

fn condition_bar(
    condition: Option<&crate::spacetimedb::ItemCondition>,
    repair_skill: Option<u8>,
) -> Markup {
    let bins = condition.map(|value| value.bins()).unwrap_or([0.0; 5]);
    let total = bins.iter().sum::<f32>().clamp(0.0, 1.0);
    let green = (1.0 - total).max(0.0);
    let label = if total <= f32::EPSILON {
        "Full durability".to_string()
    } else if repair_skill
        .is_some_and(|skill| bins.iter().take(skill.min(5) as usize).sum::<f32>() > f32::EPSILON)
    {
        "Damaged; the flashing portion can be repaired by this smith".to_string()
    } else {
        "Damaged beyond this smith's skill".to_string()
    };
    html! {
        span class="condition-bar" title=(&label) aria-label=(&label) {
            span class="condition-green" style=(format!("width:{}%", green * 100.0)) {}
            @for (index, amount) in bins.iter().enumerate() {
                @let repairable = repair_skill.is_some_and(|skill| index < skill.min(5) as usize);
                span class=(format!("condition-tier-{}{}", index + 1, if repairable { " condition-repairable" } else { "" })) style=(format!("width:{}%", amount.clamp(0.0, 1.0) * 100.0)) {}
            }
        }
    }
}

fn completed_repair_condition_bar(
    condition: Option<&crate::spacetimedb::ItemCondition>,
    smith_skill: u8,
) -> Markup {
    let Some(condition) = condition else {
        return condition_bar(None, None);
    };
    let mut repaired = condition.clone();
    let mut bins = [
        &mut repaired.tier_1,
        &mut repaired.tier_2,
        &mut repaired.tier_3,
        &mut repaired.tier_4,
        &mut repaired.tier_5,
    ];
    for amount in bins.iter_mut().take(smith_skill.min(5) as usize) {
        **amount = 0.0;
    }
    condition_bar(Some(&repaired), None)
}

fn repair_all_header(settlement: &Settlement, service_id: &str) -> Markup {
    html! {
        span class="sr-only" { "Durability" }
        form class="repair-all-form" action=(format!("/settlements/{}/{}/repair-all", settlement.id, service_id)) method="post" {
            button type="submit" class="repair-all-button" title="Entrust all eligible items for repair" aria-label="Repair all eligible items" {
                span class="repair-action-icon" aria-hidden="true" {}
            }
        }
    }
}

fn repair_submit_control(
    settlement: &Settlement,
    service_id: &str,
    inventory_item_id: u64,
    condition: Option<&crate::spacetimedb::ItemCondition>,
    skill: u8,
) -> Markup {
    let total = condition.map_or(0.0, |value| value.total());
    let repairable = condition.map_or(0.0, |value| value.repairable(skill));
    let residual = condition.map_or(0.0, |value| value.residual(skill));
    let disabled = total <= f32::EPSILON || repairable <= f32::EPSILON;
    let explanation = if total <= f32::EPSILON {
        "Item is already in full condition".to_string()
    } else if repairable <= f32::EPSILON {
        format!("All damage requires Smithing above this smith's level {skill}")
    } else if residual > f32::EPSILON {
        "Repair all damage within this smith's skill; harder damage will remain".to_string()
    } else {
        format!("Repair all damage (smith level {skill})")
    };
    html! {
        form class="row-repair-form" action=(format!("/settlements/{}/{}/repair", settlement.id, service_id)) method="post" {
            input type="hidden" name="inventory_item_id" value=(inventory_item_id);
            @if disabled {
                span class="disabled-repair-explanation" tabindex="0" title=(&explanation) aria-label=(&explanation) {
                    button type="submit" class="repair-item-button" disabled { span class="repair-action-icon" aria-hidden="true" {} }
                }
            } @else {
                button type="submit" class="repair-item-button" title=(&explanation) aria-label=(&explanation) { span class="repair-action-icon" aria-hidden="true" {} }
            }
        }
    }
}

fn repair_custody_panel(
    settlement: &Settlement,
    shop: MerchantShop,
    orders: &[crate::spacetimedb::RepairOrder],
    conditions: &[crate::spacetimedb::ItemCondition],
    items: &[crate::spacetimedb::ItemDefinition],
    now: u64,
    smith_skill: u8,
) -> Markup {
    let service_id = shop.service_id();
    let mut matching: Vec<_> = orders
        .iter()
        .filter(|order| {
            order.settlement_id == settlement.id
                && items
                    .iter()
                    .find(|item| item.id == order.item_id)
                    .is_some_and(|item| shop.stocks(item.kind))
        })
        .collect();
    matching.sort_by_key(|order| (order.submitted_at_minutes, order.id));
    html! {
        section class="repair-custody-panel" aria-label="Items entrusted for repair" {
            header class="repair-custody-header" {
                h3 { "In the smith's care" }
                span class="repair-custody-skill" title=(format!("Smithing {smith_skill}")) {
                    (stat_icon("Smithing", "skills", "smithing", false))
                    (skill_rank_bar(f32::from(smith_skill), f32::from(smith_skill), &format!("Smithing {smith_skill}")))
                }
            }
            div class="repair-custody-scroll" {
                @if matching.is_empty() { p class="text-muted small-copy" { "No items entrusted." } }
                div class="repair-custody-list" {
                    table class="trade-inventory-table repair-custody-table" {
                        thead { tr {
                            (item_type_header())
                            th scope="col" class="inventory-column-item" {
                                span class="repair-custody-item-heading" { span { "Item" }
                                    form class="repair-retrieve-all-form" data-repair-retrieve-form data-bulk-action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) method="post" {
                                        input type="hidden" name="limit" value="2";
                                        button type="submit" class="trade-transfer trade-transfer-right repair-retrieve-all" data-dynamic-transfer data-default-transfer-mode="target" data-transfer-mode="target" data-label-target="Retrieve up to two completed repairs" data-label-all="Retrieve all completed repairs" title="Retrieve up to two completed repairs" aria-label="Retrieve up to two completed repairs" { (transfer_glyph(2)) }
                                    }
                                }
                            }
                            th scope="col" class="inventory-column-durability" { "Durability" }
                            th scope="col" class="repair-column-eta" { "ETA" }
                            th scope="col" class="inventory-column-gold" title="Full repair cost (Currency)" { (currency_header("Full repair cost in Currency")) }
                        } }
                        tbody {
                        @for order in matching {
                            @let condition = conditions.iter().find(|condition| condition.inventory_item_id == order.inventory_item_id);
                            @let definition = items.iter().find(|item| item.id == order.item_id);
                            @let ready = now >= order.ready_at_minutes;
                            @let remaining = order.ready_at_minutes.saturating_sub(now);
                            tr class="trade-inventory-row trade-row-merchant repair-order-row" {
                                td class="inventory-item-type" { (item_type_icon(&order.item_id)) }
                                td class="inventory-item-name" { (item_name_with_quality(&order.item_id, definition))
                                    span class="inventory-row-actions repair-retrieve-actions" {
                                        form data-repair-retrieve-form data-single-action=(format!("/settlements/{}/{}/repairs/{}/retrieve", settlement.id, service_id, order.id)) data-bulk-action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) action=(format!("/settlements/{}/{}/repairs/{}/retrieve", settlement.id, service_id, order.id)) method="post" {
                                            input type="hidden" name="item_id" value=(&order.item_id);
                                            input type="hidden" name="limit" value="1" disabled;
                                            button type="submit" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one" data-label-one="Retrieve this completed item" data-label-target="Retrieve up to two completed matching items" data-label-all="Retrieve all completed matching items" disabled[!ready] title=(if ready { "Retrieve this completed item" } else { "Repair is still underway" }) aria-label="Retrieve this completed item" { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                                td class="inventory-durability" {
                                    @if ready { (completed_repair_condition_bar(condition, order.smith_skill)) }
                                    @else { (condition_bar(condition, Some(order.smith_skill))) }
                                }
                                td class="repair-column-eta" { @if ready { "Ready" } @else { (format!("{}h {}m", remaining / 60, remaining % 60)) } }
                                td class="inventory-gold" title="Quoted full-job cost, paid on retrieval" { (order.quoted_cost) }
                            }
                        }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn inventory_footer_controls(
    action: &str,
    target_label: &str,
    all_label: &str,
) -> Markup {
    html! { div class="inventory-footer-actions" {
        button type="button" class="trade-transfer inventory-footer-transfer" data-dynamic-transfer data-default-transfer-mode="target" data-inventory-bulk=(action) data-transfer-mode="target" data-label-target=(target_label) data-label-all=(all_label) aria-label=(target_label) title=(target_label) { (transfer_glyph(2)) }
    } }
}

fn trade_inventory_table_header(show_equipped: bool, condition_header: Option<Markup>) -> Markup {
    html! { thead { tr {
        (item_type_header())
        th scope="col" class="inventory-column-item" { "Item" }
        th scope="col" class="inventory-column-count" { "#" }
        @if show_equipped { th scope="col" class="inventory-column-equipped" title="Equipped" { (game_icon("Equipped", "check-mark")) } }
        @if let Some(condition_header) = condition_header { th scope="col" class="inventory-column-durability" { (condition_header) } }
        th scope="col" class="inventory-column-weight" title="Weight" { (game_icon("Weight", "weight")) }
        th scope="col" class="inventory-column-gold" title="Currency" { (currency_header("Currency")) }
    } } }
}

fn currency_header(label: &str) -> Markup {
    game_icon(label, "coins")
}

fn party_skills_rail(
    title: &str,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    schedule: Option<&CharacterTrainingSchedule>,
    schedule_action: Option<&str>,
    activity_preview: Option<ActivityPreviewRates>,
) -> Markup {
    let head_health = limbs.map_or(1.0, |limbs| limbs.head_health);
    let upper_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_arm_health + limbs.right_arm_health) / 2.0
    });
    let lower_health = limbs.map_or(1.0, |limbs| {
        (limbs.left_leg_health + limbs.right_leg_health) / 2.0
    });
    html! {
        (sidebar_section("", html! {
            @if let Some(skills) = skills {
                h3 class="sr-only" { (title) }
                @if let (Some(schedule), Some(action)) = (schedule, schedule_action) {
                    form class="skill-schedule" data-skill-schedule action=(action) method="post" {
                        (skills_table(title, skills, head_health, upper_health, lower_health, Some(schedule), activity_preview))
                        div class="schedule-save-status" data-schedule-save-status role="status" aria-live="polite" hidden {
                            span { "Schedule could not be saved." }
                            button type="button" data-schedule-retry { "Retry" }
                        }
                    }
                    script src="/static/training-schedule.js?v=leisure-preview-1" {}
                } @else {
                    (skills_table(title, skills, head_health, upper_health, lower_health, None, None))
                }
            } @else {
                h3 class="sidebar-header" { (title) }
                p class="text-muted small-copy" { "Skill records have not been created yet." }
            }
        }))
    }
}

fn skills_table(
    title: &str,
    skills: &CharacterSkills,
    head_health: f32,
    upper_health: f32,
    lower_health: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    activity_preview: Option<ActivityPreviewRates>,
) -> Markup {
    html! {
            table class="party-skills-table" {
                colgroup {
                    col class="party-skill-icon-column";
                    col class="party-skill-name-column";
                    @if schedule.is_some() {
                        col class="schedule-effect-column";
                        col class="schedule-effect-column";
                        col class="schedule-effect-column";
                        col class="schedule-effect-column";
                    } @else {
                        col class="party-skill-meter-column";
                    }
                }
                @if schedule.is_some() {
                    colgroup { col class="party-skill-time-column"; }
                }
                thead { tr class="schedule-context-heading" {
                        th scope="colgroup" colspan=(if schedule.is_some() { "6" } else { "3" }) class="schedule-table-title" { (title) }
                    @if schedule.is_some() {
                        th scope="col" title="Daily plan used while resting or waiting in a settlement" {
                            (schedule_header_icon("duration", "Daily allocation"))
                        }
                    }
                } }
                tbody {
                    (party_skill_row("Will", "will", Skill::Will, skills.will_hours, head_health, schedule.map(|s| s.downtime.will_minutes)))
                    (party_skill_row("Charisma", "charisma", Skill::Charisma, skills.charisma_hours, head_health, schedule.map(|s| s.downtime.charisma_minutes)))
                    (party_skill_row("Medicine", "medicine", Skill::Medicine, skills.medicine_hours, head_health, schedule.map(|s| s.downtime.medicine_minutes)))
                    (party_skill_row("Faith", "faith", Skill::Faith, skills.faith_hours, head_health, schedule.map(|s| s.downtime.faith_minutes)))
                    (party_skill_row("Melee", "melee", Skill::Melee, skills.melee_hours, upper_health, schedule.map(|s| s.downtime.melee_minutes)))
                    (party_skill_row("Ranged", "ranged", Skill::Ranged, skills.ranged_hours, upper_health, schedule.map(|s| s.downtime.ranged_minutes)))
                    (party_skill_row("Dodge", "dodge", Skill::Dodge, skills.dodge_hours, lower_health, schedule.map(|s| s.downtime.dodge_minutes)))
                    (party_skill_row("Block", "block", Skill::Block, skills.block_hours, upper_health, schedule.map(|s| s.downtime.block_minutes)))
                    (party_skill_row("Stealth", "stealth", Skill::Stealth, skills.stealth_hours, upper_health, schedule.map(|s| s.downtime.stealth_minutes)))
                    (party_skill_row("Balance", "balance", Skill::Balance, skills.balance_hours, lower_health, schedule.map(|s| s.downtime.balance_minutes)))
                    (party_skill_row("Surgeon", "surgeon", Skill::Surgeon, skills.surgeon_hours, upper_health, schedule.map(|s| s.downtime.surgeon_minutes)))
                    (party_skill_row("Smithing", "smithing", Skill::Smithing, skills.smithing_hours, upper_health, schedule.map(|s| s.downtime.smithing_minutes)))
                    @if let Some(schedule) = schedule {
                        @let preview = activity_preview.unwrap_or_default();
                        tr class="schedule-divider" { td colspan="7" {} }
                        tr class="schedule-section-heading" {
                            th colspan="2" { "Activities" }
                            th scope="col" title="Currency" { (schedule_header_icon("coins", "Currency")) }
                            th scope="col" title="Virtue" { (schedule_header_icon("scales", "Virtue")) }
                            th scope="col" title="Morale" { (schedule_header_icon("sun", "Morale")) }
                            th scope="col" title="Fatigue" { (schedule_header_icon("night-sleep", "Fatigue")) }
                            th scope="col" title="Daily allocation" { (schedule_header_icon("duration", "Daily allocation")) }
                        }
                        (schedule_special_row("Prayer", "prayer", "prayer_minutes", schedule.downtime.prayer_minutes, true, ActivityEffectRates::prayer(), None, "Recite prayers. Trains Faith at 25% speed, improves morale, and satisfies Fervor-driven daily prayer needs."))
                        (schedule_special_row("Labor", "hammer-sickle", "labor_minutes", schedule.downtime.labor_minutes, true, ActivityEffectRates::linear(preview.labor_gold_per_hour, 0.0, 0.0, LABOR_FATIGUE_PER_HOUR / FATIGUE_RESERVOIR_PER_PREVIEW_POINT), None, "Earn gold during settlement downtime from Strength and Endurance checks; trains Will at 25% speed and generates fatigue."))
                        (schedule_special_row("Thievery", "lockpicks", "thievery_minutes", schedule.downtime.thievery_minutes, true, ActivityEffectRates::linear(preview.thievery_gold_per_hour, preview.thievery_virtue_per_hour, 0.0, 0.0), None, "Settlement downtime can earn gold and risk discovery while training Stealth at 25% speed."))
                        (schedule_special_row("Raiding", "mounted-knight", "raiding_minutes", schedule.downtime.raiding_minutes, true, ActivityEffectRates::linear(preview.raiding_gold_per_hour, preview.raiding_virtue_per_hour, 0.0, 0.0), None, "Settlement downtime can earn gold and risk retaliation while training with equipped weapons and armor."))
                        @let leisure = leisure_preview(&schedule.downtime, preview.current_fatigue);
                        (schedule_special_row("Leisure", "bed", "leisure_minutes", 0, false, ActivityEffectRates::default(), Some(leisure), "Unallocated downtime first offsets baseline and activity fatigue; only surplus recovery improves morale."))
                    }
            }
        }
    }
}

fn schedule_header_icon(icon: &str, label: &str) -> Markup {
    html! { span class="schedule-header-icon" { (game_icon(label, icon)) } }
}

fn party_skill_row(
    name: &str,
    icon: &str,
    skill: Skill,
    hours: f32,
    health: f32,
    schedule_minutes: Option<u16>,
) -> Markup {
    let rank = skill.training_rank(hours);
    let effective_rank = rank * health.clamp(0.0, 1.0);
    let invested_hours = hours.max(0.0).floor() as u64;
    html! {
        tr class="party-skill-row" {
            td class="party-skill-icon-cell" { (stat_icon(name, "skills", icon, true)) }
            td class="party-skill-name" { (name) }
            td class="party-skill-meter" colspan=[schedule_minutes.map(|_| "4")] {
                (skill_rank_bar(rank, effective_rank, &format!("{invested_hours} hours invested")))
            }
            @if let Some(minutes) = schedule_minutes {
                (schedule_allocation_cell(&format!("{}_minutes", icon), minutes, true))
            }
        }
    }
}

fn skill_rank_bar(rank: f32, effective_rank: f32, title: &str) -> Markup {
    let rank = rank.clamp(0.0, 5.0);
    let effective_rank = effective_rank.clamp(0.0, rank);
    let value_left = (effective_rank / 5.0 * 100.0).clamp(2.0, 98.0);
    html! {
        div class="skill-rank-bar" title=(title) aria-label=(format!("{effective_rank:.1} out of 5")) {
            @for tier in 1..=5 {
                @let offset = (tier - 1) as f32;
                @let current = (effective_rank - offset).clamp(0.0, 1.0) * 100.0;
                @let trained = (rank - offset).clamp(0.0, 1.0) * 100.0;
                @let damaged = (trained - current).max(0.0);
                span class=(format!("skill-rank-segment skill-rank-segment-{tier}")) {
                    span class="rank-current" style=(format!("width:{current:.1}%")) {}
                    span class="rank-damage" style=(format!("left:{current:.1}%;width:{damaged:.1}%")) {}
                }
            }
            span class="skill-rank-value" style=(format!("left:{value_left:.1}%")) { (format!("{effective_rank:.1}")) }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ActivityEffectRates {
    gold_per_hour: f32,
    virtue_per_hour: f32,
    morale_per_hour: f32,
    fatigue_per_hour: f32,
    prayer_morale: bool,
}

#[derive(Clone, Copy, Debug)]
struct LeisurePreview {
    current_fatigue: f32,
    outcome: LeisureOutcome,
    fatigue_display: f32,
}

fn core_daily_schedule(schedule: &ScheduleAllocation) -> DailySchedule {
    DailySchedule {
        melee: schedule.melee_minutes,
        dodge: schedule.dodge_minutes,
        block: schedule.block_minutes,
        ranged: schedule.ranged_minutes,
        will: schedule.will_minutes,
        charisma: schedule.charisma_minutes,
        medicine: schedule.medicine_minutes,
        faith: schedule.faith_minutes,
        stealth: schedule.stealth_minutes,
        balance: schedule.balance_minutes,
        surgeon: schedule.surgeon_minutes,
        smithing: schedule.smithing_minutes,
        labor: schedule.labor_minutes,
        prayer: schedule.prayer_minutes,
        thievery: schedule.thievery_minutes,
        raiding: schedule.raiding_minutes,
    }
}

fn leisure_preview(schedule: &ScheduleAllocation, current_fatigue: f32) -> LeisurePreview {
    let outcome = settlement_leisure_outcome(
        core_daily_schedule(schedule),
        MINUTES_PER_DAY,
        current_fatigue,
    );
    let labor_fatigue = f32::from(schedule.labor_minutes) / 60.0 * LABOR_FATIGUE_PER_HOUR;
    LeisurePreview {
        current_fatigue,
        outcome,
        fatigue_display: (outcome.fatigue_delta - labor_fatigue)
            / FATIGUE_RESERVOIR_PER_PREVIEW_POINT,
    }
}

impl ActivityEffectRates {
    const fn linear(gold: f32, virtue: f32, morale: f32, fatigue: f32) -> Self {
        Self {
            gold_per_hour: gold,
            virtue_per_hour: virtue,
            morale_per_hour: morale,
            fatigue_per_hour: fatigue,
            prayer_morale: false,
        }
    }

    const fn prayer() -> Self {
        Self {
            prayer_morale: true,
            ..Self::linear(0.0, 0.0, 0.0, 0.0)
        }
    }

    fn values(self, minutes: u16) -> [f32; 4] {
        let hours = f32::from(minutes) / 60.0;
        let morale = if self.prayer_morale {
            PRAYER_MORALE_LIMIT * (1.0 - (-f32::from(minutes) / PRAYER_MORALE_SCALE_MINUTES).exp())
        } else {
            self.morale_per_hour * hours
        };
        [
            (self.gold_per_hour * hours).round(),
            self.virtue_per_hour * hours,
            morale,
            self.fatigue_per_hour * hours,
        ]
    }
}

fn activity_effect_cell(kind: &str, value: f32) -> Markup {
    let rounded = if kind == "gold" {
        value.round()
    } else {
        (value * 10.0).round() / 10.0
    };
    let state = if rounded > 0.0 {
        "positive"
    } else if rounded < 0.0 {
        "negative"
    } else {
        "neutral"
    };
    let display = if state == "neutral" {
        "0".to_string()
    } else if kind == "gold" {
        format!("{rounded:+.0}")
    } else {
        format!("{rounded:+.1}")
    };
    html! {
        td class=(format!("schedule-effect schedule-effect-{state}")) data-activity-effect=(kind) {
            (display)
        }
    }
}

fn schedule_special_row(
    label: &str,
    icon: &str,
    allocation_name: &str,
    allocation_minutes: u16,
    editable: bool,
    effects: ActivityEffectRates,
    leisure: Option<LeisurePreview>,
    description: &str,
) -> Markup {
    let values = leisure.map_or_else(
        || effects.values(allocation_minutes),
        |preview| [0.0, 0.0, preview.outcome.morale, preview.fatigue_display],
    );
    html! {
        tr class="party-skill-row schedule-special-row" title=(description)
            data-activity-row data-activity-allocation=(allocation_name)
            data-gold-rate=(effects.gold_per_hour)
            data-virtue-rate=(effects.virtue_per_hour)
            data-morale-rate=(effects.morale_per_hour)
            data-fatigue-rate=(effects.fatigue_per_hour)
            data-prayer-morale=[effects.prayer_morale.then_some("true")]
            data-prayer-morale-limit=[effects.prayer_morale.then_some(PRAYER_MORALE_LIMIT)]
            data-prayer-morale-scale=[effects.prayer_morale.then_some(PRAYER_MORALE_SCALE_MINUTES)]
            data-leisure-current-fatigue=[leisure.map(|preview| preview.current_fatigue)]
            data-leisure-baseline-fatigue=[leisure.map(|_| BASELINE_FATIGUE_PER_DAY)]
            data-leisure-labor-fatigue-rate=[leisure.map(|_| LABOR_FATIGUE_PER_HOUR)]
            data-leisure-recovery-rate=[leisure.map(|_| LEISURE_FATIGUE_RECOVERY_PER_HOUR)]
            data-leisure-morale-limit=[leisure.map(|_| LEISURE_MORALE_LIMIT)]
            data-leisure-morale-scale=[leisure.map(|_| LEISURE_MORALE_SCALE_FATIGUE)]
            data-leisure-fatigue-preview-divisor=[leisure.map(|_| FATIGUE_RESERVOIR_PER_PREVIEW_POINT)] {
            td class="party-skill-icon-cell" { (schedule_icon(label, icon)) }
            td class="party-skill-name" { strong { (label) } }
            (activity_effect_cell("gold", values[0]))
            (activity_effect_cell("virtue", values[1]))
            (activity_effect_cell("morale", values[2]))
            (activity_effect_cell("fatigue", values[3]))
            (schedule_allocation_cell(allocation_name, allocation_minutes, editable))
        }
    }
}

fn schedule_step_button(label: &str, delta: i16) -> Markup {
    html! {
        button type="button" class=(if delta < 0 { "schedule-step schedule-step-decrease" } else { "schedule-step schedule-step-increase" })
            data-schedule-step=(delta) aria-label=(label) {}
    }
}

fn schedule_allocation_cell(name: &str, minutes: u16, editable: bool) -> Markup {
    html! {
        td class="party-skill-allocation" data-schedule-value=(name) {
            @if editable {
                input type="hidden" name=(name) value=(minutes) data-schedule-input;
                (schedule_step_button("Decrease daily allocation", -15))
                span data-schedule-display tabindex="0" role="button" title="Click to enter a time such as 8, 8:30, or 830" {
                    (format_schedule_hours(minutes))
                }
                (schedule_step_button("Increase daily allocation", 15))
            } @else {
                span data-schedule-display { "0h" }
            }
        }
    }
}

fn schedule_icon(_label: &str, icon: &str) -> Markup {
    html! {
        span
            class="stat-icon schedule-special-icon"
            style=(format!("--stat-icon: url('/static/icons/game/{icon}.svg')"))
            aria-hidden="true"
        {}
    }
}

fn format_schedule_hours(minutes: u16) -> String {
    let rounded = ((u32::from(minutes) + 7) / 15) * 15;
    let hours = rounded / 60;
    let fraction = match rounded % 60 {
        0 => "",
        15 => "¼",
        30 => "½",
        45 => "¾",
        _ => unreachable!("rounded schedule minute must be a quarter hour"),
    };
    format!("{hours}{fraction}h")
}

fn character_summary_rail(capability: Option<&CharacterCapability>) -> Markup {
    let tags = capability
        .map(CharacterCapability::summary_tags)
        .unwrap_or_default();
    html! {
        (sidebar_section("Summary", html! {
            @if tags.is_empty() {
                p class="text-muted small-copy" { "No notable capabilities." }
            } @else {
                div class="character-summary-tags" aria-label="Character capability summary" {
                    @for tag in tags { span class="character-summary-tag" { (tag) } }
                }
            }
        }))
    }
}

pub(crate) fn character_stats_panel(
    character: &Character,
    capability: Option<&CharacterCapability>,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    medical: &MedicalPresentation,
) -> Markup {
    html! {
        (character_summary_rail(capability))
        (party_attributes_rail(&format!("{}'s attributes", character.name), attributes, limbs))
        (party_skills_rail(&format!("{}'s skills", character.name), skills, limbs, None, None, None))
        (medical_rail(medical, "", 0, character.id, false))
    }
}

pub(crate) fn character_visual_preview(character: &Character) -> Markup {
    visual_stage(
        "npc",
        &character.name,
        &format!("TODO: {} portrait", character.name.to_lowercase()),
    )
}

fn religion_name(religion_id: Option<&str>) -> &'static str {
    match religion_id {
        Some("western_church") => "Western Church",
        Some("roman_catholic") => "Roman Catholic",
        Some("lutheran") => "Lutheran",
        Some("reformed") => "Reformed",
        Some("anglican") => "Anglican",
        Some("protestant") => "Protestant",
        Some("eastern_orthodox") => "Eastern Orthodox",
        Some("islamic") => "Islamic",
        Some("old_faith") => "Old Faith",
        Some(_) => "Unknown faith",
        None => "None",
    }
}

fn character_bio_rail(
    character: &Character,
    religion_id: Option<&str>,
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    can_renounce: bool,
    location_path: &str,
) -> Markup {
    let virtue = if notoriety.abs() < 0.0005 {
        0.0
    } else {
        -notoriety
    };
    html! {
        (sidebar_section("Bio", html! {
            dl class="character-bio" {
                div { dt class="metric-label" { (decorative_game_icon("calendar")) span { "Age" } } dd { (character.age_years) " years" } }
                div { dt class="metric-label" { (decorative_game_icon("spiked-halo")) span { "Virtue" } } dd title="Immoral activities reduce Virtue; consequences will be added later." { (format!("{virtue:+.1}")) } }
                @if let Some(personality) = personality {
                    @let tags = personality_tags(personality);
                    @if !tags.is_empty() {
                        div { dt { "Personality" } dd class="personality-tags" {
                            @for (name, description) in tags {
                                span class="personality-tag" title=(description) { (name) }
                            }
                        } }
                    }
                }
                div class="character-religion" {
                    dt class="metric-label" { (decorative_game_icon("holy-symbol")) span { "Religion" } }
                    dd {
                        (religion_name(religion_id))
                        @if can_renounce && religion_id.is_some() {
                            form method="post" action=(format!("{location_path}/party/{}/religion/renounce", character.id)) class="character-religion-action" {
                                button type="submit" class="btn btn-danger" title="Renounce this faith" { "Renounce" }
                            }
                        }
                    }
                }
            }
        }))
    }
}

fn personality_tags(
    personality: &crate::spacetimedb::CharacterPersonality,
) -> Vec<(&'static str, &'static str)> {
    use crate::spacetimedb::{
        Conscience::*, Conviction::*, Drive::*, Nerve::*, Outlook::*, SelfRegard::*, Sociability::*,
    };
    let mut tags = Vec::new();
    match personality.nerve {
        Brave => tags.push(("Brave", "Morale loss from being outmatched ×0.5.")),
        Fearful => tags.push(("Fearful", "Morale loss from being outmatched ×2.")),
        _ => {}
    }
    match personality.drive {
        Ambitious => tags.push(("Ambitious", "Morale from victories and defeats ×1.5.")),
        Content => tags.push(("Content", "Morale from victories and defeats ×0.5.")),
        _ => {}
    }
    match personality.outlook {
        Sanguine => tags.push((
            "Sanguine",
            "Positive morale ×1.25; negative morale ×0.75; negative-event duration ×0.5.",
        )),
        Brooding => tags.push((
            "Brooding",
            "Positive morale ×0.75; negative morale ×1.25; negative-event duration ×2.",
        )),
        _ => {}
    }
    match personality.sociability {
        Gregarious => tags.push(("Gregarious", "Morale restored by allies ×1.5.")),
        Solitary => tags.push(("Solitary", "Morale restored by allies ×0.5.")),
        _ => {}
    }
    match personality.conscience {
        Compassionate => tags.push((
            "Compassionate",
            "Current morale effect ×1.0: no outcomes carry moral context yet.",
        )),
        Callous => tags.push((
            "Callous",
            "Current morale effect ×1.0: no outcomes carry moral context yet.",
        )),
        Cruel => tags.push((
            "Cruel",
            "Current morale effect ×1.0: no outcomes carry moral context yet.",
        )),
        _ => {}
    }
    match personality.self_regard {
        Proud => tags.push(("Proud", "Morale from victory ×1.5; morale from defeat ×3.")),
        Humble => tags.push(("Humble", "Morale from victories and defeats ×0.75.")),
        _ => {}
    }
    match personality.conviction {
        Zealous => tags.push(("Zealous", "Morale from religious sources and events ×1.5.")),
        Irreverent => tags.push((
            "Irreverent",
            "Morale from religious sources and events ×0.5.",
        )),
        _ => {}
    }
    tags
}

#[cfg(test)]
mod personality_tests {
    use super::*;
    use crate::spacetimedb::*;

    #[test]
    fn neutral_axes_are_omitted_from_bio_tags() {
        let personality = CharacterPersonality {
            character_id: 1,
            nerve: Nerve::Brave,
            drive: Drive::Neutral,
            outlook: Outlook::Neutral,
            sociability: Sociability::Neutral,
            conscience: Conscience::Cruel,
            self_regard: SelfRegard::Neutral,
            conviction: Conviction::Neutral,
        };
        let tags = personality_tags(&personality);
        assert_eq!(
            tags.iter().map(|tag| tag.0).collect::<Vec<_>>(),
            ["Brave", "Cruel"]
        );
    }

    #[test]
    fn every_visible_tag_explains_its_numeric_morale_effect() {
        let profiles = [
            CharacterPersonality {
                character_id: 1,
                nerve: Nerve::Brave,
                drive: Drive::Ambitious,
                outlook: Outlook::Sanguine,
                sociability: Sociability::Gregarious,
                conscience: Conscience::Compassionate,
                self_regard: SelfRegard::Proud,
                conviction: Conviction::Zealous,
            },
            CharacterPersonality {
                character_id: 2,
                nerve: Nerve::Fearful,
                drive: Drive::Content,
                outlook: Outlook::Brooding,
                sociability: Sociability::Solitary,
                conscience: Conscience::Callous,
                self_regard: SelfRegard::Humble,
                conviction: Conviction::Irreverent,
            },
            CharacterPersonality {
                character_id: 3,
                nerve: Nerve::Neutral,
                drive: Drive::Neutral,
                outlook: Outlook::Neutral,
                sociability: Sociability::Neutral,
                conscience: Conscience::Cruel,
                self_regard: SelfRegard::Neutral,
                conviction: Conviction::Neutral,
            },
        ];

        for profile in &profiles {
            for (tag, description) in personality_tags(profile) {
                assert!(
                    description.contains('×'),
                    "{tag} tooltip lacks a numeric multiplier: {description}"
                );
            }
        }
    }
}

fn strategic_condition_rail(
    condition: Option<&CharacterStrategicCondition>,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
) -> Markup {
    let Some(condition) = condition else {
        return html! {};
    };
    let percent = |value: f32| format!("{:.0}%", value.max(0.0) * 100.0);
    let fear_fill = (condition.fear.clamp(0.0, 1.0) * 100.0).round();
    let bonus_fill = if condition.morale_bonus_cap > 0.0 {
        (condition.morale_bonus / condition.morale_bonus_cap * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    }
    .round();
    let meter_style = format!("--morale-fear: {fear_fill}%; --morale-bonus: {bonus_fill}%");
    html! {
        (sidebar_section("Condition", html! {
            div class="morale-meter" tabindex="0" style=(meter_style) aria-label=(format!(
                "Morale {:.1}; fear {}; personal share of party morale restoration {:.1}%",
                condition.morale,
                percent(condition.fear),
                condition.morale_bonus * 100.0,
            )) {
                div class="morale-meter-heading" {
                    strong class="metric-label" { (decorative_game_icon("sun")) span { "Morale" } }
                    span { (format!("{:+.1}", condition.morale)) }
                }
                div class="morale-meter-track" aria-hidden="true" {
                    span class="morale-meter-fear" {}
                    span class="morale-meter-neutral" {}
                    span class="morale-meter-bonus" {}
                }
                div class="morale-meter-labels" {
                    span { "100% fear" }
                    span { "Neutral" }
                    span { (format!("{:.1}% ally lift", condition.morale_bonus * 100.0)) }
                }
                div class="morale-source-popup" role="tooltip" {
                    strong { "Morale sources" }
                    @if morale_sources.is_empty() {
                        p { "No current morale effects." }
                    } @else {
                        ul {
                            @for source in morale_sources {
                                li class=(if source.magnitude >= 0.0 { "positive" } else { "negative" }) {
                                    span { (&source.label) }
                                    strong { (format!("{:+.1}", source.magnitude)) }
                                }
                            }
                        }
                    }
                }
            }
            div class="fervor-meter" tabindex="0" style=(format!("--fervor: {:.0}%", condition.fervor.clamp(0.0, 1.0) * 100.0)) aria-label=(format!("Fervor {}", percent(condition.fervor))) {
                div class="fervor-meter-heading" {
                    strong class="metric-label" { (decorative_game_icon("holy-symbol")) span { "Fervor" } }
                    span { (percent(condition.fervor)) }
                }
                div class="fervor-meter-track" aria-hidden="true" { span {} }
                div class="fervor-meter-labels" {
                    span { "Calm" }
                    span { "Fervent" }
                    span { "Frenzy" }
                }
                p class="fervor-help" role="tooltip" {
                    "Faith, a strong same-faith cohort, and surplus morale raise Fervor. Party Charisma restrains it. Higher Fervor makes conviction demands and settlement incidents more likely."
                }
            }
            dl class="character-bio strategic-condition-summary" {
                div { dt { "Status" } dd { (condition.status.to_uppercase()) } }
                div { dt class="metric-label" { (decorative_game_icon("coma")) span { "Incapacitation" } } dd { (percent(condition.incapacitation)) } }
                div { dt class="metric-label" { (decorative_game_icon("broken-heart")) span { "Pain" } } dd { (percent(condition.pain)) } }
                div { dt class="metric-label" { (decorative_game_icon("bleeding-wound")) span { "Blood loss" } } dd { (percent(condition.blood_loss)) } }
                div { dt class="metric-label" { (decorative_game_icon("terror")) span { "Fear" } } dd { (percent(condition.fear)) } }
                div { dt class="metric-label" { (decorative_game_icon("night-sleep")) span { "Fatigue" } } dd { (percent(condition.fatigue)) } }
                div { dt class="metric-label" { (decorative_game_icon("meal")) span { "Hunger" } } dd { (percent(condition.hunger)) } }
                div { dt class="metric-label" { (decorative_game_icon("water-drop")) span { "Thirst" } } dd { (percent(condition.thirst)) } }
                div { dt class="metric-label" { (decorative_game_icon("bread")) span { "Food" } } dd { (format!("{:.1} travel days", condition.food_days.max(0.0))) } }
                div { dt class="metric-label" { (decorative_game_icon("waterskin")) span { "Water" } } dd { (format!("{:.1} travel days / {:.1} L capacity", condition.water_days.max(0.0), condition.water_capacity_ml as f32 / 1_000.0)) } }
                div { dt class="metric-label" { (decorative_game_icon("check-mark")) span { "Check effectiveness" } } dd { (percent(condition.check_multiplier)) } }
            }
        }))
    }
}

fn medical_rail(
    medical: &MedicalPresentation,
    location_path: &str,
    doctor_id: u64,
    target_id: u64,
    allow_treatment: bool,
) -> Markup {
    html! {
        (sidebar_section("Symptoms", html! {
            @if medical.symptoms.is_empty(){p class="text-muted small-copy" { "No visible symptoms." }}@else{p class="medical-symptoms" {(medical.symptoms.join(" · "))}}
        }))
        @if let Some(vitals)=medical.vitals {
            (sidebar_section("Vitals", html! {
                div class="humour-vitals" aria-label="Four humours" {
                    (humour_bar("Sanguine", "Blood and circulation", vitals.sanguine, "sanguine"))
                    (humour_bar("Phlegmatic", "Breath", vitals.phlegmatic, "phlegmatic"))
                    (humour_bar("Choleric", "Heat and digestion", vitals.choleric, "choleric"))
                    (humour_bar("Melancholic", "Sense and reason", vitals.melancholic, "melancholic"))
                }
                @if !medical.diagnoses.is_empty(){div class="medical-diagnoses" {h3 {"Diagnosed conditions"}@for diagnosis in &medical.diagnoses {article {strong {(diagnosis.period_name)} span class="condition-stage" {" — "(diagnosis.stage)} p class="small-copy" {(diagnosis.contagion)} @if allow_treatment && diagnosis.treatable {form method="post" action=(format!("{location_path}/party/{target_id}/disease/{}/treat",diagnosis.infection_id)){input type="hidden" name="doctor_id" value=(doctor_id);button type="submit" class="btn btn-primary" {"Treat"}}}}}}}
            }))
        }
    }
}

fn humour_bar(name: &str, meaning: &str, value: f32, family: &str) -> Markup {
    let pct = (value.clamp(0.0, 1.0) * 100.0).round();
    html! {div class="humour-vital" {div class="humour-heading" {strong {(name)} span {(format!("{pct:.0}%"))}} div class="humour-track" role="meter" aria-label=(format!("{name}: {meaning}, {pct:.0} percent function")) aria-valuemin="0" aria-valuemax="100" aria-valuenow=(pct) {span class="humour-current" style=(format!("width:{pct:.0}%")) {} span class=(format!("humour-impairment impairment-{family}")) style=(format!("left:{pct:.0}%;width:{:.0}%",100.0-pct)) aria-hidden="true" {}}}}
}

fn party_attributes_rail(
    title: &str,
    attributes: Option<&CharacterAttributes>,
    limbs: Option<&CharacterLimbs>,
) -> Markup {
    let Some(attributes) = attributes else {
        return html! {};
    };
    let head_health = limbs.map_or(1.0, |limbs| limbs.head_health);
    let chest_health = limbs.map_or(1.0, |limbs| limbs.chest_health);
    let stomach_health = limbs.map_or(1.0, |limbs| limbs.stomach_health);
    let left_arm_health = limbs.map_or(1.0, |limbs| limbs.left_arm_health);
    let right_arm_health = limbs.map_or(1.0, |limbs| limbs.right_arm_health);
    let left_leg_health = limbs.map_or(1.0, |limbs| limbs.left_leg_health);
    let right_leg_health = limbs.map_or(1.0, |limbs| limbs.right_leg_health);
    html! {
        (sidebar_section(title, html! {
            div class="party-attributes-list" aria-label="Character attributes" {
                (attribute_group("Head", head_health, &[
                    ("Intelligence", "intelligence", attributes.intelligence),
                    ("Instinct", "instinct", attributes.instinct),
                    ("Eyesight", "eyesight", attributes.eyesight),
                    ("Hearing", "hearing", attributes.hearing),
                ]))
                (attribute_group("Chest", chest_health, &[
                    ("Endurance", "endurance", attributes.endurance),
                ]))
                (attribute_group("Stomach", stomach_health, &[
                    ("Immunity", "immunity", attributes.immunity),
                    ("Gut", "gut", attributes.gut),
                ]))
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left arm", "limb-left", left_arm_health, &[
                        ("Strength", "strength-arm", attributes.left_arm_strength),
                        ("Agility", "agility-arm", attributes.left_arm_agility),
                    ]))
                    (limb_attribute_column("Right arm", "limb-right", right_arm_health, &[
                        ("Strength", "strength-arm", attributes.right_arm_strength),
                        ("Agility", "agility-arm", attributes.right_arm_agility),
                    ]))
                }
                div class="limb-attribute-pair" {
                    (limb_attribute_column("Left leg", "limb-left", left_leg_health, &[
                        ("Strength", "strength-leg", attributes.left_leg_strength),
                        ("Agility", "agility-leg", attributes.left_leg_agility),
                    ]))
                    (limb_attribute_column("Right leg", "limb-right", right_leg_health, &[
                        ("Strength", "strength-leg", attributes.right_leg_strength),
                        ("Agility", "agility-leg", attributes.right_leg_agility),
                    ]))
                }
            }
        }))
    }
}

fn limb_attribute_column(
    name: &str,
    side: &str,
    health: f32,
    rows: &[(&str, &str, f32)],
) -> Markup {
    attribute_group_with_labels(name, health, rows, false, Some(side))
}

fn attribute_group(name: &str, health: f32, rows: &[(&str, &str, f32)]) -> Markup {
    attribute_group_with_labels(name, health, rows, true, None)
}

fn attribute_group_with_labels(
    name: &str,
    health: f32,
    rows: &[(&str, &str, f32)],
    show_labels: bool,
    side: Option<&str>,
) -> Markup {
    let health = health.clamp(0.0, 1.0);
    let health_width = health * 100.0;
    let damage_width = (1.0 - health) * 100.0;
    html! {
        div class=(match side {
            Some(side) => format!("attribute-group limb-attribute-column {side}"),
            None => "attribute-group".to_owned(),
        }) {
            div class="attribute-group-heading" { (name) }
            div class="attribute-health-bar" title=(format!("{name} health: {health_width:.0}%")) {
                span class="attribute-health-current" style=(format!("width:{health_width:.1}%")) {}
                span class="attribute-health-damage" style=(format!("left:{health_width:.1}%;width:{damage_width:.1}%")) {}
            }
            @for (attribute_name, icon, value) in rows {
                (attribute_row(attribute_name, icon, *value, health, show_labels))
            }
        }
    }
}

fn attribute_row(name: &str, icon: &str, value: f32, health: f32, show_label: bool) -> Markup {
    let effective_value = value * health.clamp(0.0, 1.0);
    let current_width = (effective_value.clamp(0.0, 5.0) / 5.0) * 100.0;
    let damage_width = ((value - effective_value).max(0.0) / 5.0) * 100.0;
    html! {
        div class=(if show_label { "party-attribute-row" } else { "party-attribute-row party-attribute-icon-only" }) {
            (stat_icon(name, "attributes", icon, show_label))
            @if show_label { span class="party-attribute-name" { (name) } }
            div class="attribute-rank-bar" title=(format!("{effective_value:.1}")) {
                span class="rank-current" style=(format!("width:{current_width:.1}%")) {}
                span class="rank-damage" style=(format!("left:{current_width:.1}%;width:{damage_width:.1}%")) {}
            }
        }
    }
}

fn stat_icon(label: &str, category: &str, icon: &str, decorative: bool) -> Markup {
    let path = stat_icon_path(category, icon);
    html! {
        span
            class=(format!("stat-icon stat-icon-{icon}"))
            style=(format!("--stat-icon: url('{path}')"))
            role=[(!decorative).then_some("img")]
            aria-label=[(!decorative).then_some(label)]
            title=[(!decorative).then_some(label)]
            aria-hidden=[decorative.then_some("true")]
        {}
    }
}

pub(crate) fn visual_stage(kind: &str, title: &str, placeholder: &str) -> Markup {
    html! {
        figure class=(format!("service-visual service-visual-{}", kind)) {
            div class="service-visual-placeholder" role="img" aria-label=(placeholder) {
                @if kind == "map" {
                    span class="visual-symbol" { (decorative_game_icon("treasure-map")) }
                    span class="visual-label" { "Map placeholder" }
                } @else {
                    span class="visual-symbol" { (title.chars().next().unwrap_or('?')) }
                    span class="visual-label" { "Portrait placeholder" }
                }
            }
        }
    }
}

pub(crate) fn party_portrait_overlay(
    party_members: &[Character],
    active_character: Option<&Character>,
    location_path: &str,
    selected_character_id: Option<u64>,
) -> Markup {
    let members: Vec<&Character> = if party_members.is_empty() {
        active_character.into_iter().collect()
    } else {
        party_members.iter().collect()
    };
    let leader_id = members.first().map(|member| member.id);

    html! {
        @if !members.is_empty() {
            div class="party-portrait-overlay" aria-label="Active party" {
                div data-party-portrait-members {
                @if active_character.is_some() {
                    div class="party-portrait party-inventory-portrait" title="Party inventory" {
                        a class="party-portrait-select" href=(format!("{}/party-inventory", location_path)) {
                            span class="party-portrait-initial party-chest-face" { (game_icon("Party inventory", "knapsack")) }
                        }
                    }
                }
                @for member in members {
                    @let is_active = active_character.is_some_and(|character| character.id == member.id);
                    @let can_remove = Some(member.id) != leader_id;
                    div class=(format!("party-portrait{}{}", if selected_character_id == Some(member.id) { " active" } else { "" }, if !member.alive { " dead" } else { "" }))
                        data-character-id=(member.id)
                        data-character-alive=(member.alive)
                        data-active-character[is_active]
                        title=(&member.name) {
                        a class="party-portrait-select"
                            href=(if is_active {
                                format!("{}/party/{}", location_path, member.id)
                            } else {
                                format!("{}/party/{}/stats", location_path, member.id)
                            })
                            title=(format!("Inspect {}", member.name)) {
                            (incapacitation_wheel(member.id))
                            span class="party-portrait-initial" {
                                span class="party-portrait-face" { (member.name.chars().next().unwrap_or('?')) }
                                span class="party-portrait-name" { (&member.name) @if !member.alive { " (dead)" } }
                            }
                        }
                        @if member.alive && active_character.is_some_and(|character| character.alive) {
                        span class="party-portrait-actions" aria-label=(format!("Actions for {}", member.name)) {
                            a href=(format!("{}/party/{}/inventory", location_path, member.id))
                                class="party-portrait-action"
                                title=(if is_active { "Open inventory and discard items".to_string() } else { format!("Compare inventory with {}", member.name) }) {
                                span class="party-action-icon"
                                    style="--party-action-icon: url('/static/icons/game/knapsack.svg')"
                                    role="img" aria-label="Inventory" {}
                            }
                            @if can_remove {
                                form method="post" action=(format!("{}/party/{}/remove", location_path, member.id)) {
                                    button type="submit" class=(if is_active { "party-portrait-action party-member-remove party-member-leave" } else { "party-portrait-action party-member-remove party-member-kick-request" })
                                        title=(if is_active { "Leave party".to_string() } else { format!("Request to remove {} from the party", member.name) })
                                        aria-label=(if is_active { "Leave party".to_string() } else { format!("Request to remove {} from the party", member.name) }) {
                                        span aria-hidden="true" { "×" }
                                    }
                                }
                            }
                        }
                        }
                    }
                }
                }
            }
        }
    }
}

fn incapacitation_wheel(character_id: u64) -> Markup {
    html! {
        span class="incapacitation-wheel"
            data-strategic-condition-wheel=(character_id)
            role="img"
            aria-label="Loading strategic condition"
            title="Loading strategic condition" {}
    }
}

/// Shared chat panel. Local conversations are live; the remaining channel
/// filters are present so their messages can join the same stream as their
/// backends become available.
pub(crate) fn settlement_chat_area(location: &str, active_character: Option<&Character>) -> Markup {
    chat_area(location, active_character, None, None, &[])
}

pub(crate) fn settlement_chat_area_with_info(
    location: &str,
    active_character: Option<&Character>,
    info_messages: &[String],
) -> Markup {
    chat_area(location, active_character, None, None, info_messages)
}

fn player_chat_area(subject: &Character, active_character: &Character) -> Markup {
    let context = ("player", subject.id.to_string());
    chat_area(
        &subject.name,
        Some(active_character),
        None,
        Some(context),
        &[],
    )
}

fn settlement_service_chat_area(
    location: &str,
    active_character: Option<&Character>,
    settlement_id: &str,
    service_id: &str,
) -> Markup {
    let subject_id = format!("{settlement_id}:{service_id}");
    chat_area(
        location,
        active_character,
        Some((settlement_id, service_id)),
        Some(("npc", subject_id)),
        &[],
    )
}

fn chat_area(
    location: &str,
    _active_character: Option<&Character>,
    service_context: Option<(&str, &str)>,
    local_context: Option<(&str, String)>,
    info_messages: &[String],
) -> Markup {
    html! {
        section class="settlement-chat" aria-label="Settlement chat"
            data-service-quest-settlement=[service_context.map(|context| context.0)]
            data-service-quest-id=[service_context.map(|context| context.1)]
            data-local-chat-kind=[local_context.as_ref().map(|context| context.0)]
            data-local-chat-subject=[local_context.as_ref().map(|context| context.1.as_str())] {
            div class="settlement-chat-resize" role="separator" aria-label="Resize chat"
                aria-orientation="horizontal" aria-valuemin="128" aria-valuemax="640"
                aria-valuenow="184" tabindex="0" title="Drag to resize chat" {
                span aria-hidden="true" {}
            }
            div class="settlement-chat-filters" role="group" aria-label="Visible chat channels" {
                @for (channel, label) in [
                    ("local", "Local"),
                    ("party", "Party"),
                    ("settlement", "Settlement"),
                    ("dm", "DMs"),
                    ("guild", "Guild"),
                    ("info", "Info"),
                ] {
                    label class=(format!("chat-channel-filter chat-channel-filter-{channel}")) title=(label) {
                        input type="checkbox" checked data-chat-filter=(channel)
                            aria-label=(label) title=(label);
                    }
                }
            }
            div class="settlement-chat-messages" aria-live="polite" {
                @if local_context.is_none() { div class="chat-system-message" data-chat-channel="info" {
                    span class="chat-timestamp" { "[--:--] " }
                    " Select a local character or settlement service to begin talking."
                } }
                @for message in info_messages {
                    div class="chat-system-message" data-chat-channel="info" {
                        span class="chat-timestamp" { "[--:--] " }
                        (message)
                    }
                }
            }
            div class="settlement-chat-composer" {
                input type="text" name="body" disabled[local_context.is_none()]
                    aria-label="Local message"
                    placeholder=(format!("Message {location} (Local)"));
                button type="button" class="btn btn-primary btn-icon" disabled[local_context.is_none()]
                    aria-label="Send message" {
                    "➤"
                }
            }
        }
    }
}

fn merchant_offers_rail(title: &str, placeholder_offers: &[&str]) -> Markup {
    html! {
        (sidebar_section(title, html! {
            p class="text-muted small-copy" { "TODO: merchant inventory and prices are not available yet." }
            table class="trade-inventory-table" {
                (trade_inventory_table_header(false, None))
                tbody {
                @for offer in placeholder_offers {
                    tr class="trade-inventory-row trade-row-merchant"
                        title="TODO: buying requires merchant inventory, pricing, and trade reducers" {
                        td class="inventory-item-type" { (game_icon("Item type: placeholder", "help")) }
                        td class="inventory-item-name" {
                            (offer)
                            button type="button" class="trade-transfer trade-transfer-right" disabled
                                aria-label=(format!("Buy {}", offer))
                                title="TODO: buying requires merchant inventory, pricing, and trade reducers" { "▶" }
                        }
                        td class="inventory-count" { "1" }
                        td class="inventory-weight" { "—" }
                        td class="inventory-gold" { "—" }
                    }
                }
                }
            }
        }))
    }
}

fn inventory_rail(
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    trade_action: Option<(&str, &str)>,
    show_repair: bool,
) -> Markup {
    let title = active_character
        .map(|character| format!("{}'s inventory", character.name))
        .unwrap_or_else(|| "Your inventory".to_string());

    html! {
        (sidebar_section(&title, html! {
            @if inventory.is_empty() {
                p class="text-muted small-copy" { "No items carried." }
            } @else {
                table class="trade-inventory-table" {
                    (trade_inventory_table_header(false, None))
                    tbody {
                    @for item in inventory {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        tr class=(if trade_action.is_some() { "trade-inventory-row" } else { "trade-inventory-row inventory-row-readonly" }) {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" {
                                (item_name_with_quality(&item.item_id, definition))
                                @if let Some((action, tooltip)) = trade_action {
                                button type="button" class="trade-transfer trade-transfer-left" disabled
                                    aria-label=(format!("{} {}", action, item.item_id))
                                    title=(tooltip) { "◀" }
                                }
                                @if show_repair {
                                span class="repair-placeholder"
                                    style="--repair-icon: url('/static/icons/game/hammer-nails.svg')"
                                    role="img"
                                    aria-label=(format!("Repair {}", item.item_id))
                                    title="TODO: repairs require durability, pricing, and repair reducers" {}
                                }
                            }
                            td class="inventory-count" { (item.qty) }
                            td class="inventory-weight" { "—" }
                            td class="inventory-gold" { "—" }
                        }
                    }
                }
                }
            }
        }))
    }
}

pub fn rest_result_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    theme: &str,
    at_inn: bool,
    summary: &RestSummary,
) -> Markup {
    service_page(
        settlement,
        if at_inn { "inn" } else { "religion" },
        if at_inn { "The Inn" } else { "Church" },
        if at_inn { "Innkeeper" } else { "Priest" },
        "",
        active_character,
        inventory,
        items,
        party_members,
        logged_in_as,
        theme,
        None,
        Some(summary),
    )
}

fn rest_service_menu(
    location: &str,
    settlement_id: &str,
    kind: &str,
    default_minutes: Option<u64>,
    summary: Option<&RestSummary>,
) -> Markup {
    html! {
    section class="rest-service-menu" aria-label=(format!("{} rest service", location)) {
        div class="rest-service-heading" { strong { "Rest" } }
        @if kind == "inn" {
            p class="rest-service-copy" { "A bed costs 1 gold per day. Injuries are tended before any downtime." }
        } @else {
            p class="rest-service-copy" { "Sanctuary is freely offered to those down on their luck. Injuries are tended before any downtime." }
        }
        form action=(format!("/settlements/{settlement_id}/rest/{kind}")) method="post" {
                @let minutes = default_minutes.unwrap_or(0);
                @let unit = if minutes >= 1_440 { "days" } else { "hours" };
                @let value = if unit == "days" { minutes.div_ceil(1_440) } else { minutes.div_ceil(60) };
                (rest_duration_control("settlement-rest", value, unit, "Rest duration"))
                /*
                div class="rest-days-control" {
                    button type="button" class="rest-days-step rest-days-decrease" aria-label="Decrease rest days"
                        onclick="const input=this.parentElement.querySelector('input'); input.value=Math.max(0, Number(input.value || 0)-1); input.dispatchEvent(new Event('input', {bubbles:true}));" { "−" }
                    input type="number" name="days" value="0" min="0" max="365" aria-label="Rest days"
                        oninput="this.form.querySelector('[type=submit]').disabled=Number(this.value || 0) <= 0;";
                    span class="rest-days-unit" { "days" }
                    button type="button" class="rest-days-step rest-days-increase" aria-label="Increase rest days"
                        onclick="const input=this.parentElement.querySelector('input'); input.value=Math.min(Number(input.max || 365), Number(input.value || 0)+1); input.dispatchEvent(new Event('input', {bubbles:true}));" { "+" }
                    button type="button" class="rest-days-heal" aria-label="Rest until fully healed"
                        title="Set the rest duration needed to fully heal"
                        onclick=(format!("const input=this.parentElement.querySelector('input'); input.value={}; input.dispatchEvent(new Event('input', {{bubbles:true}}));", healing_days.unwrap_or(0))) { "Until healed" }
                }
                */
                button type="submit" class="btn btn-primary btn-small btn-block" data-rest-submit disabled[value == 0] { "Rest" }
        }
        @if let Some(summary) = summary {
            div class="rest-summary-overlay" role="dialog" aria-modal="true" aria-labelledby="rest-summary-title" {
                section class="rest-summary" {
                    div class="rest-summary-heading" {
                        strong id="rest-summary-title" { "Rest summary" }
                        a href=(format!("/settlements/{settlement_id}/{}", if kind == "inn" { "inn" } else { "religion" })) class="rest-summary-close" aria-label="Close rest summary" { "×" }
                    }
                    p { @if summary.minutes >= 1_440 { (summary.minutes / 1_440) " day" @if summary.minutes / 1_440 != 1 { "s" } " passed." } @else { (summary.minutes / 60) " hour" @if summary.minutes / 60 != 1 { "s" } " passed." } }
                    @if summary.gold_spent > 0 { p { (summary.gold_spent) " gold paid." } }
                    @if summary.gold_earned > 0 { p { (summary.gold_earned) " gold earned from activities." } }
                    @if summary.notoriety_gained > 0.0 { p class="schedule-effect-negative" { (format!("-{:.1}", summary.notoriety_gained)) " Virtue from activities." } }
                    @if summary.healed.is_empty() { p { "No injuries needed tending." } } @else {
                        p { "Healed:" }
                        ul { @for (part, amount) in &summary.healed { li { (part) ": +" (format!("{amount:.0}%")) } } }
                    }
                    @if summary.trained.is_empty() { p { "No time remained for downtime." } } @else {
                        p { "Training:" }
                        ul { @for (skill, hours) in &summary.trained { li { (skill) ": +" (format!("{hours:.2}h")) } } }
                    }
                }
            }
        }
        }
    }
}

fn days_to_full_health(limbs: &CharacterLimbs) -> u16 {
    let lowest_health = [
        limbs.left_arm_health,
        limbs.right_arm_health,
        limbs.left_leg_health,
        limbs.right_leg_health,
        limbs.head_health,
        limbs.chest_health,
        limbs.stomach_health,
    ]
    .into_iter()
    .fold(1.0_f32, f32::min);
    ((1.0 - lowest_health).max(0.0) / 0.05).ceil() as u16
}

fn rest_default_minutes(
    limbs: Option<&CharacterLimbs>,
    stats: Option<&CharacterStats>,
    condition: Option<&CharacterCondition>,
    field_repair_minutes: u64,
    smith_wait_minutes: u64,
) -> Option<u64> {
    let healing_days = limbs.map(days_to_full_health).unwrap_or(0);
    let healing_minutes = u64::from(healing_days) * 1_440;
    let fatigue_minutes = stats
        .map(|stats| ((stats.calories_used / 2_000.0) * 1_440.0).ceil() as u64)
        .unwrap_or(0);
    let blood_recovery_minutes = condition.map_or(0, blood_recovery_minutes);
    (limbs.is_some() || stats.is_some() || condition.is_some()).then_some(
        healing_minutes
            .max(fatigue_minutes)
            .max(blood_recovery_minutes)
            .saturating_add(field_repair_minutes)
            .max(smith_wait_minutes),
    )
}

/// This must match the strategic module's `BLOOD_RECOVERY_FRACTION_PER_DAY`.
const BLOOD_RECOVERY_FRACTION_PER_DAY: f32 = 0.01;

fn blood_recovery_minutes(condition: &CharacterCondition) -> u64 {
    if condition.maximum_blood_ml <= 0.0 {
        return 0;
    }
    let missing_fraction = ((condition.maximum_blood_ml - condition.current_blood_ml)
        / condition.maximum_blood_ml)
        .clamp(0.0, 1.0);
    (missing_fraction / BLOOD_RECOVERY_FRACTION_PER_DAY * 1_440.0).ceil() as u64
}
#[cfg(test)]
mod tests {
    use super::{
        CharacterCondition, LocationKind, MerchantShop, repair_custody_panel,
        repair_submit_control, rest_default_minutes,
    };

    #[test]
    fn location_kind_rejects_unknown_path_segments() {
        assert_eq!("quest".parse(), Ok(LocationKind::Quest));
        assert!("merchant".parse::<LocationKind>().is_err());
    }

    #[test]
    fn rest_recommendation_includes_blood_recovery() {
        let condition = CharacterCondition {
            character_id: 1,
            body_weight_kg: 70.0,
            current_blood_ml: 4_900.0,
            maximum_blood_ml: 5_000.0,
            religion_id: None,
        };

        assert_eq!(
            rest_default_minutes(None, None, Some(&condition), 0, 0),
            Some(2_880)
        );
    }

    #[test]
    fn canonical_imported_religions_have_ui_labels() {
        for (id, label) in [
            ("roman_catholic", "Roman Catholic"),
            ("lutheran", "Lutheran"),
            ("reformed", "Reformed"),
            ("anglican", "Anglican"),
            ("protestant", "Protestant"),
            ("eastern_orthodox", "Eastern Orthodox"),
            ("islamic", "Islamic"),
        ] {
            assert_eq!(religion_name(Some(id)), label);
        }
    }
    fn settlement() -> Settlement {
        Settlement {
            id: "viabundus-1".into(),
            name: "Lübeck".into(),
            coord_x: 10.0,
            coord_y: 53.0,
            population_level: 4,
            population_estimate: 12_000,
            industries: adventuresim_world_schema::InferredIndustryProfile::new(vec![
                adventuresim_world_schema::IndustryEvidence::Fallback(
                    adventuresim_world_schema::FallbackIndustry::CroplandGrain,
                ),
            ])
            .unwrap(),
            scene_key: "hills".into(),
            religion_id: "western_church".into(),
            source_node_id: Some(1),
        }
    }

    #[test]
    fn disabled_repair_explanation_is_hoverable_and_focusable() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.0,
            tier_2: 0.0,
            tier_3: 0.0,
            tier_4: 0.2,
            tier_5: 0.0,
        };
        let rendered =
            repair_submit_control(&settlement(), "weapons", 4, Some(&condition), 3).into_string();
        assert!(rendered.contains("disabled-repair-explanation"));
        assert!(rendered.contains("tabindex=\"0\""));
        assert!(rendered.contains("All damage requires Smithing"));
        assert!(rendered.contains("disabled"));
    }

    #[test]
    fn smith_player_actions_keep_sell_and_repair_in_one_hover_area() {
        let repair = repair_submit_control(&settlement(), "weapons", 4, None, 3);
        let rendered =
            merchant_sell_repair_controls(4, "torch", 2, 3, 1, true, Some(repair)).into_string();

        assert!(rendered.starts_with("<div class=\"inventory-row-actions smith-player-actions\">"));
        assert_eq!(rendered.matches("data-merchant-sell=\"").count(), 1);
        assert!(rendered.contains("data-dynamic-transfer"));
        assert!(rendered.contains("data-default-transfer-mode=\"one\""));
        assert!(rendered.contains("data-label-target=\"Sell surplus torch\""));
        assert!(rendered.contains("data-label-all=\"Sell all torch\""));
        assert!(rendered.contains("row-repair-form"));
        assert_eq!(rendered.matches("smith-player-actions").count(), 1);
    }

    #[test]
    fn equipped_smith_items_retain_the_repair_action_without_sell_controls() {
        let repair = repair_submit_control(&settlement(), "weapons", 4, None, 3);
        let rendered =
            merchant_sell_repair_controls(4, "sword", 10, 1, 0, false, Some(repair)).into_string();

        assert!(rendered.contains("smith-player-actions"));
        assert!(rendered.contains("row-repair-form"));
        assert!(!rendered.contains("data-merchant-sell"));
    }

    #[test]
    fn durability_bar_uses_qualitative_copy_and_marks_smith_repairable_damage() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.1,
            tier_2: 0.0,
            tier_3: 0.2,
            tier_4: 0.1,
            tier_5: 0.0,
        };
        let rendered = condition_bar(Some(&condition), Some(3)).into_string();
        assert!(rendered.contains("condition-repairable"));
        for tier in 1..=5 {
            assert!(rendered.contains(&format!("condition-tier-{tier}")));
        }
        assert!(rendered.contains("flashing portion can be repaired"));
        assert!(!rendered.contains("condition-number"));
        assert!(!rendered.contains("% condition"));
    }

    #[test]
    fn durable_item_names_expose_quality_color_and_description() {
        let definition = crate::spacetimedb::ItemDefinition {
            id: "commissioned_sword".into(),
            weight: 1.0,
            slot: ItemSlot::AnyHolding,
            kind: crate::spacetimedb::ItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 4,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
        };

        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("item-quality-4"));
        assert!(rendered.contains("knightly commission"));
    }

    #[test]
    fn completed_repair_bar_projects_the_condition_before_retrieval() {
        let condition = crate::spacetimedb::ItemCondition {
            inventory_item_id: 4,
            tier_1: 0.1,
            tier_2: 0.2,
            tier_3: 0.0,
            tier_4: 0.0,
            tier_5: 0.0,
        };

        let rendered = completed_repair_condition_bar(Some(&condition), 3).into_string();
        assert!(rendered.contains("Full durability"));
        assert!(rendered.contains("width:100%"));
    }

    #[test]
    fn smith_player_inventory_uses_the_compact_seven_column_table() {
        let rendered = trade_inventory_table(
            true,
            Some(repair_all_header(&settlement(), "weapons")),
            html! {},
        )
        .into_string();
        assert!(rendered.contains("smith-player-inventory-table"));
        assert!(rendered.contains("inventory-column-type"));
        assert!(rendered.contains("aria-label=\"Item type\""));
        assert!(rendered.contains("inventory-column-durability"));
        assert!(rendered.contains("Repair all eligible items"));
        assert!(rendered.contains("class=\"sr-only\">Durability</span>"));
        assert!(!rendered.contains("durability-header-label"));
    }

    #[test]
    fn skill_meter_and_schedule_use_segmented_rank_and_text_time_controls() {
        let meter = skill_rank_bar(3.5, 2.75, "Skill test").into_string();
        for tier in 1..=5 {
            assert!(meter.contains(&format!("skill-rank-segment-{tier}")));
        }
        let allocation = schedule_allocation_cell("smithing_minutes", 75, true).into_string();
        assert!(allocation.contains("data-schedule-input"));
        assert!(allocation.contains("data-schedule-display"));
        assert!(allocation.contains("Click to enter a time such as 8, 8:30, or 830"));
        assert!(!allocation.contains("type=\"range\""));
        assert!(!allocation.contains("schedule-handle"));
    }

    #[test]
    fn schedule_table_uses_compact_accessible_icon_headers() {
        let skills = CharacterSkills {
            character_id: 1,
            melee_hours: 0.0,
            dodge_hours: 0.0,
            block_hours: 0.0,
            ranged_hours: 0.0,
            will_hours: 0.0,
            charisma_hours: 0.0,
            medicine_hours: 0.0,
            faith_hours: 0.0,
            stealth_hours: 0.0,
            balance_hours: 0.0,
            surgeon_hours: 0.0,
            smithing_hours: 0.0,
        };
        let schedule = CharacterTrainingSchedule {
            character_id: 1,
            downtime: crate::spacetimedb::ScheduleAllocation::default(),
            travel: crate::spacetimedb::ScheduleAllocation::default(),
        };
        let rendered = skills_table("Your skills", &skills, 1.0, 1.0, 1.0, Some(&schedule), None)
            .into_string();

        assert!(rendered.contains(
            "scope=\"colgroup\" colspan=\"6\" class=\"schedule-table-title\">Your skills"
        ));
        assert_eq!(rendered.matches("<colgroup>").count(), 2);
        assert_eq!(
            rendered.matches("aria-label=\"Daily allocation\"").count(),
            2
        );
        for label in ["Currency", "Virtue", "Morale", "Fatigue"] {
            assert!(rendered.contains(&format!("aria-label=\"{label}\"")));
        }
        assert!(rendered.contains("/static/icons/game/coins.svg"));
        assert!(!rendered.contains(">Gold</th>"));
        assert!(!rendered.contains(">Virt.</th>"));

        let rail = party_skills_rail(
            "Your skills",
            Some(&skills),
            None,
            Some(&schedule),
            Some("/schedule"),
            None,
        )
        .into_string();
        assert!(!rail.contains("class=\"sidebar-header\">Your skills"));
        assert!(rail.contains("<h3 class=\"sr-only\">Your skills</h3>"));
        assert!(rail.contains("data-schedule-save-status"));
        assert!(rail.contains("role=\"status\" aria-live=\"polite\" hidden"));
        assert!(rail.contains("data-schedule-retry>Retry</button>"));
    }

    #[test]
    fn activity_rows_show_signed_daily_effects_instead_of_allocation_bars() {
        let rendered = schedule_special_row(
            "Thievery",
            "market",
            "thievery_minutes",
            120,
            true,
            ActivityEffectRates::linear(2.0, -1.0, 0.0, 0.0),
            None,
            "Test activity",
        )
        .into_string();
        for effect in ["gold", "virtue", "morale", "fatigue"] {
            assert!(rendered.contains(&format!("data-activity-effect=\"{effect}\"")));
        }
        assert!(rendered.contains("schedule-effect-positive"));
        assert!(rendered.contains(">+4</td>"));
        assert!(rendered.contains("schedule-effect-negative"));
        assert!(rendered.contains(">-2.0</td>"));
        assert!(!rendered.contains("schedule-allocation-fill"));
        assert!(!rendered.contains("schedule-special-track"));
    }

    #[test]
    fn server_rendered_effects_normalize_negative_zero() {
        let rendered = activity_effect_cell("fatigue", -0.0006).into_string();
        assert!(rendered.contains("schedule-effect-neutral"));
        assert!(rendered.contains(">0</td>"));
        assert!(!rendered.contains("-0.0"));

        let negative = activity_effect_cell("fatigue", -0.06).into_string();
        assert!(negative.contains("schedule-effect-negative"));
        assert!(negative.contains(">-0.1</td>"));
    }

    #[test]
    fn leisure_and_labor_previews_decompose_the_shared_fatigue_outcome() {
        let schedule = ScheduleAllocation {
            labor_minutes: 240,
            melee_minutes: 720,
            ..Default::default()
        };
        let leisure = leisure_preview(&schedule, 0.0);
        assert_eq!(leisure.outcome.leisure_hours, 8.0);
        assert_eq!(leisure.outcome.fatigue_delta, 0.0);
        assert_eq!(leisure.outcome.morale, 0.0);
        assert_eq!(leisure.fatigue_display, -2.0);
        assert_eq!(
            ActivityEffectRates::linear(
                0.0,
                0.0,
                0.0,
                LABOR_FATIGUE_PER_HOUR / FATIGUE_RESERVOIR_PER_PREVIEW_POINT,
            )
            .values(schedule.labor_minutes)[3],
            2.0
        );
        let rendered = schedule_special_row(
            "Leisure",
            "inn",
            "leisure_minutes",
            0,
            false,
            ActivityEffectRates::default(),
            Some(leisure),
            "Test leisure",
        )
        .into_string();
        for attribute in [
            "data-leisure-baseline-fatigue",
            "data-leisure-labor-fatigue-rate",
            "data-leisure-recovery-rate",
            "data-leisure-morale-limit",
            "data-leisure-morale-scale",
            "data-leisure-fatigue-preview-divisor",
        ] {
            assert!(rendered.contains(attribute));
        }
        assert!(rendered.contains(">-2.0</td>"));
    }

    #[test]
    fn equipment_checkbox_is_enabled_only_for_equippable_items() {
        let inventory = InventoryItem {
            id: 7,
            character_id: 9,
            item_id: "sword".into(),
            qty: 1,
        };
        let mut definition = crate::spacetimedb::ItemDefinition {
            id: "sword".into(),
            weight: 1.0,
            slot: ItemSlot::AnyHolding,
            kind: crate::spacetimedb::ItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 3,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
        };
        let enabled = equipment_checkbox(&inventory, Some(&definition), false).into_string();
        assert!(enabled.contains("data-equipment-toggle"));
        assert!(!enabled.contains(" disabled"));
        definition.slot = ItemSlot::None;
        let disabled = equipment_checkbox(&inventory, Some(&definition), false).into_string();
        assert!(disabled.contains(" disabled"));
    }

    #[test]
    fn merchant_stock_table_keeps_quantity_weight_and_gold_columns() {
        let rendered = trade_inventory_table(false, None, html! {}).into_string();
        assert!(rendered.contains("<colgroup>"));
        assert!(rendered.contains("inventory-column-count"));
        assert!(rendered.contains("inventory-column-type"));
        assert!(rendered.contains("inventory-column-weight"));
        assert!(rendered.contains("inventory-column-gold"));
        assert!(rendered.contains("title=\"Currency\""));
        assert!(rendered.contains("aria-label=\"Currency\""));
        assert!(rendered.contains("/static/icons/game/coins.svg"));
    }

    #[test]
    fn inventory_type_header_and_row_share_the_first_column() {
        let rendered = trade_inventory_table(
            false,
            None,
            html! {
                tr class="trade-inventory-row" {
                    td class="inventory-item-type" { (item_type_icon("arming_sword")) }
                    td class="inventory-item-name" { "Arming sword" }
                    td { "1" } td { "1" } td { "12" }
                }
            },
        )
        .into_string();
        let header = rendered.find("inventory-column-type").unwrap();
        let item_header = rendered.find("inventory-column-item").unwrap();
        let type_cell = rendered.find("inventory-item-type").unwrap();
        let item_cell = rendered.find("inventory-item-name").unwrap();
        assert!(header < item_header && type_cell < item_cell);
        assert!(rendered.contains("/static/icons/game/broadsword.svg"));
    }

    #[test]
    fn smith_custody_panel_shows_only_matching_service_orders() {
        let orders = [
            crate::spacetimedb::RepairOrder {
                id: 1,
                owner_character_id: 9,
                inventory_item_id: 11,
                item_id: "sword".into(),
                settlement_id: "viabundus-1".into(),
                smith_skill: 3,
                submitted_at_minutes: 0,
                ready_at_minutes: 10,
                target_condition: 1.0,
                quoted_cost: 12,
            },
            crate::spacetimedb::RepairOrder {
                id: 2,
                owner_character_id: 9,
                inventory_item_id: 12,
                item_id: "cuirass".into(),
                settlement_id: "viabundus-1".into(),
                smith_skill: 3,
                submitted_at_minutes: 0,
                ready_at_minutes: 10,
                target_condition: 1.0,
                quoted_cost: 24,
            },
        ];
        let items = [
            crate::spacetimedb::ItemDefinition {
                id: "sword".into(),
                weight: 1.0,
                slot: ItemSlot::AnyHolding,
                kind: crate::spacetimedb::ItemKind::Weapon,
                base_value: None,
                nutrition_kcal: 0.0,
                water_capacity_ml: 0,
                quality: 3,
                durability_yield: 0.0,
                durability_fracture: 0.0,
                durability_wear: 0.0,
                durability_failure_share: 0.0,
                edge_sensitivity: 0.0,
                handling_sensitivity: 0.0,
            },
            crate::spacetimedb::ItemDefinition {
                id: "cuirass".into(),
                weight: 1.0,
                slot: ItemSlot::Chest,
                kind: crate::spacetimedb::ItemKind::Armor,
                base_value: None,
                nutrition_kcal: 0.0,
                water_capacity_ml: 0,
                quality: 3,
                durability_yield: 0.0,
                durability_fracture: 0.0,
                durability_wear: 0.0,
                durability_failure_share: 0.0,
                edge_sensitivity: 0.0,
                handling_sensitivity: 0.0,
            },
        ];
        let weapons = repair_custody_panel(
            &settlement(),
            MerchantShop::Weapons,
            &orders,
            &[],
            &items,
            0,
            4,
        )
        .into_string();
        let armor = repair_custody_panel(
            &settlement(),
            MerchantShop::Armor,
            &orders,
            &[],
            &items,
            0,
            3,
        )
        .into_string();
        assert!(weapons.contains("sword"));
        assert!(!weapons.contains("cuirass"));
        assert!(weapons.contains("repair-custody-table"));
        assert!(weapons.contains("Smithing 4"));
        assert!(weapons.contains("stat-icon-smithing"));
        for tier in 1..=5 {
            assert!(weapons.contains(&format!("skill-rank-segment-{tier}")));
        }
        assert!(weapons.contains("repair-custody-item-heading"));
        assert!(weapons.contains("Durability"));
        assert!(weapons.contains("ETA"));
        assert!(weapons.contains("Full repair cost"));
        assert!(weapons.contains("repair-retrieve-all"));
        assert!(weapons.contains("Retrieve up to two completed matching items"));
        assert!(!weapons.to_lowercase().contains("affordable prefix"));
        assert!(weapons.contains("/repairs/1/retrieve"));
        assert!(weapons.contains(">12<"));
        assert!(!weapons.contains("Target "));
        assert!(armor.contains("cuirass"));
        assert!(!armor.contains("sword"));
    }

    #[test]
    fn aliases_are_deduplicated_and_do_not_repeat_the_canonical_name() {
        let aliases = [
            SettlementAlias {
                id: "1".into(),
                settlement_id: "viabundus-1".into(),
                name: "Lubeke".into(),
                prefix: None,
                language: Some("deu".into()),
            },
            SettlementAlias {
                id: "2".into(),
                settlement_id: "viabundus-1".into(),
                name: "Lübeck".into(),
                prefix: None,
                language: None,
            },
        ];

        assert_eq!(settlement_alias_labels(&settlement(), &aliases), ["Lubeke"]);
    }

    #[test]
    fn english_settlement_description_is_preferred_deterministically() {
        let descriptions = [
            SettlementDescription {
                id: "1".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::Settlement,
                language: Some("deu".into()),
                body: "Deutsch".into(),
            },
            SettlementDescription {
                id: "2".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::City,
                language: Some("eng".into()),
                body: "English city".into(),
            },
            SettlementDescription {
                id: "3".into(),
                settlement_id: "viabundus-1".into(),
                kind: SettlementDescriptionKind::Settlement,
                language: Some("eng".into()),
                body: "English settlement".into(),
            },
        ];

        assert_eq!(
            preferred_settlement_description(&descriptions)
                .unwrap()
                .body,
            "English settlement"
        );
    }

    #[test]
    fn settlement_overview_renders_enrichment_as_escaped_text() {
        let aliases = [SettlementAlias {
            id: "1".into(),
            settlement_id: "viabundus-1".into(),
            name: "Lubeke".into(),
            prefix: None,
            language: Some("deu".into()),
        }];
        let descriptions = [SettlementDescription {
            id: "1".into(),
            settlement_id: "viabundus-1".into(),
            kind: SettlementDescriptionKind::Settlement,
            language: Some("deu".into()),
            body: "Burg & Markt <alt>".into(),
        }];

        let markup = settlement_overview_page(
            &settlement(),
            &aliases,
            &descriptions,
            None,
            &[],
            None,
            "system",
        )
        .into_string();

        assert!(markup.contains("Also known as"));
        assert!(markup.contains("Lubeke"));
        assert!(markup.contains("Historical description — German"));
        assert!(markup.contains("Burg &amp; Markt &lt;alt&gt;"));
        assert!(!markup.contains("<alt>"));
    }
    use super::*;

    fn quest_destination() -> TravelDestination {
        TravelDestination {
            id: "quest-location".to_string(),
            name: "Bandit camp".to_string(),
            description: "A camp beside the road.".to_string(),
            summary: Some("Active quest".to_string()),
            travel_action: "/quests/quest-location/travel".to_string(),
            distance_m: 1_000,
            journey_minutes: 48,
            camp_stop_minutes: Vec::new(),
            camp_forecasts: Vec::new(),
            quest_in_progress: true,
            active_quest_route: false,
            turn_in_ready: false,
        }
    }

    #[test]
    fn active_quest_destination_has_red_status_badge() {
        let destination = quest_destination();

        let markup = map_destination_list(&[destination], None, "/locations/settlement/test/map")
            .into_string();

        assert!(markup.contains("destination-quest-badge"));
        assert!(markup.contains("Active quest destination"));
        assert!(!markup.contains("destination-turn-in-badge"));
    }

    #[test]
    fn quest_location_travel_does_not_offer_unavailable_provisioning() {
        let destination = quest_destination();
        let markup =
            map_destination_detail(Some(&destination), true, false, None, None, false, "/map")
                .into_string();

        assert!(markup.contains("value=\"underprovisioned\""));
        assert!(!markup.contains("Provision and travel"));
        assert!(!markup.contains("Provision forecast"));
    }

    #[test]
    fn chat_uses_one_stream_with_all_channel_filters() {
        let markup = chat_area("Lubeck", None, None, None, &[]).into_string();

        assert!(!markup.contains("role=\"tablist\""));
        for channel in ["local", "party", "settlement", "dm", "guild", "info"] {
            assert!(
                markup.contains(&format!("data-chat-filter=\"{channel}\"")),
                "missing {channel} filter"
            );
        }
        assert!(markup.contains("data-chat-channel=\"info\""));
        assert!(!markup.contains("chat-channel-badge"));
        for label in ["Local", "Party", "Settlement", "DMs", "Guild", "Info"] {
            assert!(markup.contains(&format!("aria-label=\"{label}\" title=\"{label}\"")));
            assert!(!markup.contains(&format!(">{label}</")));
        }
    }

    #[test]
    fn chat_palette_meets_contrast_across_every_supported_theme() {
        fn linear_channel(channel: u8) -> f64 {
            let channel = f64::from(channel) / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        }

        fn luminance([red, green, blue]: [u8; 3]) -> f64 {
            0.2126 * linear_channel(red)
                + 0.7152 * linear_channel(green)
                + 0.0722 * linear_channel(blue)
        }

        fn contrast(first: [u8; 3], second: [u8; 3]) -> f64 {
            let (lighter, darker) = if luminance(first) > luminance(second) {
                (luminance(first), luminance(second))
            } else {
                (luminance(second), luminance(first))
            };
            (lighter + 0.05) / (darker + 0.05)
        }

        fn mix(accent: [u8; 3], text: [u8; 3], accent_percent: u16) -> [u8; 3] {
            std::array::from_fn(|index| {
                let mixed = u16::from(accent[index]) * accent_percent
                    + u16::from(text[index]) * (100 - accent_percent);
                ((mixed + 50) / 100) as u8
            })
        }

        // Dark themes use the lightest possible 88% panel composite (over
        // white); light themes use the darkest possible composite (over
        // black). This brackets the image content beneath the translucent chat.
        let themes = [
            (
                "Dark Arcanum",
                [46, 49, 67],
                [200, 202, 208],
                [154, 158, 176],
                [96, 165, 250],
                [251, 191, 36],
                [215, 169, 239],
                [52, 211, 153],
            ),
            (
                "Fraktur Nocturne",
                [60, 49, 44],
                [241, 227, 207],
                [205, 185, 157],
                [125, 159, 197],
                [213, 166, 76],
                [213, 167, 237],
                [120, 173, 114],
            ),
            (
                "Fraktur Texturina",
                [216, 209, 190],
                [42, 31, 20],
                [74, 60, 44],
                [58, 106, 138],
                [184, 134, 11],
                [116, 66, 141],
                [74, 124, 63],
            ),
            (
                "Imperial Crimson",
                [217, 213, 204],
                [26, 26, 26],
                [61, 61, 61],
                [26, 74, 138],
                [196, 136, 11],
                [113, 63, 140],
                [45, 106, 48],
            ),
            (
                "Northern Frost",
                [211, 215, 220],
                [28, 40, 51],
                [52, 73, 94],
                [46, 109, 164],
                [212, 160, 23],
                [115, 66, 147],
                [39, 174, 96],
            ),
            (
                "Renaissance Gold",
                [216, 209, 190],
                [42, 31, 20],
                [74, 60, 44],
                [58, 106, 138],
                [184, 134, 11],
                [123, 63, 145],
                [74, 124, 63],
            ),
            (
                "Verdant Chronicle",
                [218, 214, 202],
                [26, 60, 26],
                [45, 90, 45],
                [74, 122, 106],
                [184, 115, 51],
                [116, 66, 141],
                [58, 122, 58],
            ),
        ];
        for (theme, surface, primary, secondary, info, gold, dm, success) in themes {
            let channels = [
                ("Local", primary),
                ("Party", mix(info, primary, 40)),
                ("Settlement", mix(gold, primary, 35)),
                ("DM", mix(dm, primary, 35)),
                ("Guild", mix(success, primary, 40)),
                ("Info", secondary),
            ];
            let distinct = channels
                .iter()
                .map(|(_, color)| color)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                distinct.len(),
                channels.len(),
                "{theme} channels must remain visually distinct"
            );
            for (channel, color) in channels {
                assert!(
                    contrast(color, surface) >= 4.5,
                    "{theme} {channel} does not meet WCAG AA text contrast"
                );
            }
        }
    }

    #[test]
    fn chat_css_keeps_fallbacks_and_mobile_message_space() {
        let css = include_str!("../../static/css/strategic.css");
        let utilities = include_str!("../../static/css/utilities.css");
        let trade_script = include_str!("../../static/party-trade.js");
        let fallback = css
            .find("background: rgb(33 21 15 / 88%);")
            .expect("chat needs a background fallback");
        let enhanced = css
            .find("background: color-mix(in srgb, var(--panel-bg) 88%, transparent);")
            .expect("chat should derive its translucent surface from the theme");

        assert!(fallback < enhanced);
        assert!(css.contains("background: color-mix(in srgb, var(--header-bg) 86%, transparent);"));
        assert!(css.contains(".chat-channel-filter input::after"));
        assert!(css.contains("border-radius: 0.05rem;"));
        assert!(css.contains("outline: 2px solid var(--text-primary);"));
        assert!(css.contains("0 0 0 2px var(--panel-bg)"));
        assert!(css.contains(
            "--chat-party-color: color-mix(in srgb, var(--info) 40%, var(--text-primary));"
        ));
        assert!(css.contains("--chat-settlement-color: color-mix(in srgb, var(--gold-color) 35%, var(--text-primary));"));
        assert!(css.contains("--chat-dm-color: color-mix(in srgb, var(--icon-instinct, #7b3f91) 35%, var(--text-primary));"));
        assert!(css.contains(
            "--chat-guild-color: color-mix(in srgb, var(--success) 40%, var(--text-primary));"
        ));
        for variable in ["local", "party", "settlement", "dm", "guild", "info"] {
            assert!(css.contains(&format!("var(--chat-{variable}-color)")));
        }
        assert!(!css.contains(".chat-channel-badge"));
        assert!(css.contains("@media (max-width: 768px)"));
        assert!(css.contains("flex-wrap: nowrap;"));
        assert!(css.contains("min-height: 10rem;"));
        assert!(css.contains(".repair-custody-list { margin-top: auto; }"));
        assert!(css.contains("max-height: 50%;"));
        assert!(css.contains("@keyframes repairable-damage-pulse"));
        assert!(css.contains("@media (prefers-reduced-motion: reduce)"));
        let repairable_rule = css
            .split(".condition-repairable {")
            .nth(1)
            .and_then(|tail| tail.split('}').next())
            .expect("repairable condition segments need a style rule");
        assert!(!repairable_rule.contains("background-image"));
        assert!(!repairable_rule.contains("box-shadow"));
        for tier in 1..=5 {
            assert!(css.contains(&format!(".condition-tier-{tier}")));
            assert!(css.contains(&format!(".item-quality-{tier}")));
        }
        assert!(css.contains("color-mix(in srgb, var(--quality-color) 50%, var(--text-primary))"));
        assert!(css.contains("filter: brightness(1.15)"));
        assert!(css.contains("0%, 58%, 82%, 100%"));
        assert!(css.contains("66%, 74%"));
        assert!(!css.contains("left: -7rem;"));
        assert!(css.contains(".smith-wares-scroll .trade-inventory-table"));
        assert!(css.contains("overflow-x: hidden;"));
        assert!(css.contains("col.inventory-column-item { width: auto; }"));
        assert!(css.contains(".smith-player-inventory-table"));
        assert!(css.contains("width: 3.65rem;"));
        assert!(css.contains(".repair-custody-item-heading"));
        assert!(css.contains("width: calc(100% + 1.65rem);"));
        assert!(css.contains("right: -1.65rem;"));
        assert!(utilities.contains(".inventory-row-actions.smith-player-actions"));
        assert!(utilities.contains(".smith-wares-scroll .inventory-row-actions"));
        assert!(utilities.contains(".smith-wares-scroll .inventory-footer-actions"));
        assert!(utilities.contains("grid-template-columns:repeat(2,1.35rem)"));
        assert!(utilities.contains(
            ".right-sidebar .inventory-row-actions.smith-player-actions { left:-3.15rem; }"
        ));
        assert!(
            utilities
                .contains(".smith-player-actions .row-repair-form { position:static; order:0;")
        );
        assert!(trade_script.contains("if (stockRow) changeTradeDraftCount(stockRow, amount);"));
        assert!(trade_script.contains("function applyDynamicTransferModifiers(event)"));
        assert!(trade_script.contains("event.key === \"Shift\" || event.key === \"Control\""));
        assert!(trade_script.contains("controlKey ? \"all\""));
    }

    #[test]
    fn schedule_and_equipment_scripts_use_the_new_interactions() {
        let schedule = include_str!("../../static/training-schedule.js");
        let equipment = include_str!("../../static/equipment-toggle.js");
        let live_regions = include_str!("../../static/live-regions.js");
        let css = include_str!("../../static/css/strategic.css");
        assert!(schedule.contains("function parseClock(value)"));
        assert!(schedule.contains("input.type = 'text'"));
        assert!(schedule.contains("/^\\d{3,4}$/"));
        assert!(schedule.contains("Math.round(wanted / STEP) * STEP"));
        assert!(schedule.contains("function renderActivityPreview(row, minutes)"));
        assert!(schedule.contains("function calculateLeisurePreview"));
        assert!(schedule.contains("row.dataset.leisureFatiguePreviewDivisor"));
        assert!(schedule.contains("function mountSchedules(root = document)"));
        assert!(schedule.contains("'strategic-live-regions-refreshed'"));
        assert!(schedule.contains("event.detail.regions.includes('left-sidebar')"));
        assert!(schedule.contains("function createLatestSaveQueue(send"));
        assert!(schedule.contains("data-schedule-pending"));
        assert!(schedule.contains("retry()"));
        assert!(schedule.contains("data-schedule-save-status"));
        assert!(schedule.contains("data-schedule-retry"));
        assert!(schedule.contains("strategic-live-refresh-requested"));
        assert!(schedule.contains("schedule-effect-positive"));
        assert!(!schedule.contains("scheduleDrag"));
        assert!(!schedule.contains("travel_"));
        assert!(equipment.contains("'/api/equipment'"));
        assert!(equipment.contains("window.location.reload()"));
        assert!(live_regions.contains("const scrollOffsets = (selector)"));
        assert!(live_regions.contains("region.scrollTop = offsets.top"));
        assert!(live_regions.contains("replaced.includes(\"left-sidebar\")"));
        assert!(live_regions.contains("scheduleEditorIsPending"));
        assert!(live_regions.contains("const schedulePendingAtStart = scheduleEditorIsPending()"));
        assert!(live_regions.contains("!schedulePendingAtStart && !scheduleEditorIsPending()"));
        assert!(css.contains(".schedule-time-input {"));
        assert!(css.contains("position: absolute;"));
        assert!(css.contains(".schedule-save-status"));
    }

    #[test]
    fn low_medicine_medical_html_contains_no_hidden_payload() {
        let presentation = crate::medical::MedicalPresentation {
            symptoms: vec!["coughing"],
            vitals: None,
            diagnoses: Vec::new(),
        };
        let markup = medical_rail(&presentation, "/location", 1, 2, true).into_string();
        assert!(markup.contains("coughing"));
        for forbidden in ["Vitals", "influenza", "infection_id", "disease", "humour-"] {
            assert!(!markup.contains(forbidden), "leaked {forbidden}: {markup}");
        }
    }

    #[test]
    fn humour_meter_has_text_and_aria_not_color_alone() {
        let markup = humour_bar("Phlegmatic", "Breath", 0.4, "phlegmatic").into_string();
        assert!(markup.contains("Phlegmatic"));
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("Breath, 40 percent function"));
    }
}
