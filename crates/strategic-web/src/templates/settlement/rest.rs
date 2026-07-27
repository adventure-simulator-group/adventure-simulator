use maud::{Markup, html};

use super::trade::service_page;
use crate::spacetimedb::{
    Character, CharacterCondition, CharacterLimbs, CharacterStats, FoodLot, InventoryItem,
    Settlement,
};
use crate::templates::decorative_game_icon;

pub struct RestSummary {
    pub minutes: u64,
    pub full_board_gold_spent: u32,
    pub additional_gold_spent: u32,
    pub gold_earned: u32,
    pub notoriety_gained: f32,
    pub healed: Vec<(String, f32)>,
    pub trained: Vec<(String, f32)>,
}
pub(crate) fn party_rest_menu(
    action: &str,
    id_prefix: &str,
    heading: &str,
    submit_label: &str,
    default_minutes: u64,
    scheduled_wake_minute: Option<u16>,
    soap_preview: SoapRestPreview,
) -> Markup {
    html! {
        div class="rest-service-heading" { strong { (heading) } }
        form action=(action) method="post" {
            (wake_time_rest_duration_control(
                id_prefix,
                default_minutes.max(1),
                "hours",
                1,
                Some(default_minutes.max(1)),
                scheduled_wake_minute,
            ))
            button type="submit" class="btn btn-primary btn-small btn-block" data-rest-submit {
                (submit_label)
            }
        }
        (soap_wash_preview(soap_preview))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoapRestPreview {
    pub total_units: u32,
    pub personal_units: u32,
    pub shared_units: u32,
    pub available_units: u32,
    pub alcohol_available: bool,
    pub alcohol_will_be_consumed: bool,
}

fn soap_wash_preview(preview: SoapRestPreview) -> Markup {
    let soap_tooltip = if preview.total_units > 0 {
        let format_soap = |points: u32| {
            let units = points as f32 / adventuresim_core::filth::SOAP_CLEANSING_CAPACITY as f32;
            format!("{units:.2}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        };
        let source = if preview.personal_units > 0 && preview.shared_units > 0 {
            format!(
                " ({} personal, {} shared)",
                format_soap(preview.personal_units),
                format_soap(preview.shared_units)
            )
        } else if preview.shared_units > 0 {
            " (shared)".to_string()
        } else {
            " (personal)".to_string()
        };
        format!(
            "Washing before rest will use {} soft soap{}. Soap is also a surgical supply.",
            format_soap(preview.total_units),
            source
        )
    } else if preview.available_units > 0 {
        "Soft soap is available, but none is needed for washing before this rest. Soap is also a surgical supply."
            .to_string()
    } else {
        "No soft soap is available for washing before rest. Soap is also a surgical supply."
            .to_string()
    };
    let alcohol_tooltip = if preview.alcohol_will_be_consumed {
        "Alcohol is available and will be consumed automatically during nightly rest."
    } else if preview.alcohol_available {
        "Alcohol is available, but no eligible character will drink it. Temperate characters do not drink."
    } else {
        "No alcohol is available for automatic consumption during nightly rest."
    };
    html! {
        div class="rest-consumable-indicators" aria-label="Automatic rest supplies" {
            span class=(if preview.available_units > 0 { "rest-consumable-indicator available" } else { "rest-consumable-indicator unavailable" })
                role="img" tabindex="0" aria-label="Soap" title=(soap_tooltip) {
                (decorative_game_icon("water-drop"))
            }
            span class=(if preview.alcohol_will_be_consumed { "rest-consumable-indicator available" } else { "rest-consumable-indicator unavailable" })
                role="img" tabindex="0" aria-label="Alcohol" title=(alcohol_tooltip) {
                (decorative_game_icon("beer-stein"))
            }
        }
    }
}

pub fn rest_result_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    at_inn: bool,
    summary: &RestSummary,
    soap_preview: SoapRestPreview,
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
        food_lots,
        party_members,
        logged_in_as,
        None,
        Some(summary),
        soap_preview,
    )
}

pub(super) fn rest_service_menu(
    location: &str,
    settlement_id: &str,
    kind: &str,
    default_minutes: Option<u64>,
    summary: Option<&RestSummary>,
    soap_preview: SoapRestPreview,
) -> Markup {
    html! {
    section class="rest-service-menu" aria-label=(format!("{} rest service", location))
        data-live-refresh-url=(format!(
            "/settlements/{settlement_id}/{}",
            if kind == "inn" { "inn" } else { "religion" }
        ))
        title=(if kind == "inn" { "A bed costs 1 coin per day. Injuries are tended before downtime." } else { "Sanctuary is free. Injuries are tended before downtime." }) {
        div class="rest-service-heading" { strong { "Rest" } }
        @if kind == "inn" {
            p class="rest-service-copy" { "2 coin / day · meals + water + treatment included" }
        } @else {
            p class="rest-service-copy" { "Free · treatment included" }
        }
        form action=(format!("/settlements/{settlement_id}/rest/{kind}")) method="post" {
                @let minutes = default_minutes.unwrap_or(0);
                @let unit = if minutes >= 1_440 { "days" } else { "hours" };
                @let initial_minutes = if minutes == 0 { 1_440 } else { minutes.max(1_440) };
                (settlement_rest_duration_control(initial_minutes, unit))
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
                button type="submit" class="btn btn-primary btn-small btn-block" data-rest-submit disabled[unit == "hours"] title="Rest for the selected duration" {
                    (decorative_game_icon("night-sleep"))
                    span class="sr-only" { "Rest" }
                }
        }
        (soap_wash_preview(soap_preview))
        @if let Some(summary) = summary {
            div class="rest-summary-overlay" role="dialog" aria-modal="true"
                aria-labelledby="rest-summary-title" tabindex="-1" data-rest-summary {
                section class="rest-summary" {
                    div class="rest-summary-heading" {
                        strong id="rest-summary-title" { "Rest summary" }
                        a href=(format!("/settlements/{settlement_id}/{}", if kind == "inn" { "inn" } else { "religion" })) class="rest-summary-close" aria-label="Close rest summary" { "×" }
                    }
                    p { (format_rest_duration(summary.minutes)) " passed." }
                    @if summary.full_board_gold_spent > 0 {
                        p { (summary.full_board_gold_spent) " coin paid for full board." }
                    }
                    @if summary.additional_gold_spent > 0 {
                        @if summary.full_board_gold_spent > 0 {
                            p { (summary.additional_gold_spent) " additional coin spent during rest." }
                        } @else {
                            p { (summary.additional_gold_spent) " coin paid." }
                        }
                    }
                    @if summary.gold_earned > 0 { p { (summary.gold_earned) " coin earned from activities." } }
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

fn settlement_rest_duration_control(initial_minutes: u64, unit: &str) -> Markup {
    wake_time_rest_duration_control("settlement-rest", initial_minutes, unit, 1_440, None, None)
}

fn wake_time_rest_duration_control(
    id_prefix: &str,
    initial_minutes: u64,
    unit: &str,
    minimum_minutes: u64,
    default_minutes: Option<u64>,
    scheduled_wake_minute: Option<u16>,
) -> Markup {
    let hours_active = unit == "hours";
    let wake_id = format!("{id_prefix}-wake-time");
    let value = if hours_active {
        format!("{:02}:{:02}", initial_minutes / 60, initial_minutes % 60)
    } else {
        initial_minutes.div_ceil(1_440).max(1).to_string()
    };
    html! {
        div class="rest-duration-control settlement-rest-duration" data-rest-duration data-wake-time
            data-rest-minimum-minutes=(minimum_minutes)
            data-rest-default-minutes=[default_minutes]
            data-rest-scheduled-wake-minute=[scheduled_wake_minute] {
            div class="rest-duration-units" role="radiogroup" aria-label="Rest duration" {
                label class=(if hours_active { "rest-duration-unit active" } else { "rest-duration-unit" }) {
                    input type="radio" name="unit" value="hours" checked[hours_active] {}
                    "Hours"
                }
                label class=(if !hours_active { "rest-duration-unit active" } else { "rest-duration-unit" }) {
                    input type="radio" name="unit" value="days" checked[!hours_active] {}
                    "Days"
                }
            }
            div class="rest-wake-time" data-wake-time-panel aria-disabled=(!hours_active) {
                div class="rest-wake-heading" {
                    label for=(&wake_id) { "Wake time" }
                    output for=(&wake_id) data-wake-time-output { "08:00" }
                }
                input id=(&wake_id) type="range" min="0" max="1439" step="60" value="480"
                    aria-label="Wake time" aria-valuetext="08:00" disabled[!hours_active] data-wake-time-slider;
            }
            div class="rest-days-control" {
                button type="button" class="rest-days-step rest-days-decrease" aria-label="Decrease rest duration" data-rest-step="-1" { "−" }
                input type=(if hours_active { "text" } else { "number" }) name="duration"
                    value=(value)
                    inputmode=(if hours_active { "text" } else { "numeric" })
                    pattern="[0-9]+:[0-5][0-9]" min="1" max="365" step="1"
                    aria-label="Rest duration" data-rest-duration-input;
                span class="rest-days-unit" data-rest-unit-label { (unit) }
                button type="button" class="rest-days-step rest-days-increase" aria-label="Increase rest duration" data-rest-step="1" { "+" }
            }
            input type="hidden" name="requested_minutes" disabled[!hours_active] data-rest-exact-minutes;
        }
    }
}

fn format_rest_duration(minutes: u64) -> String {
    let days = minutes / 1_440;
    let hours = minutes % 1_440 / 60;
    let minutes = minutes % 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days} day{}", if days == 1 { "" } else { "s" }));
    }
    if hours > 0 {
        parts.push(format!("{hours} hour{}", if hours == 1 { "" } else { "s" }));
    }
    if minutes > 0 || parts.is_empty() {
        parts.push(format!(
            "{minutes} minute{}",
            if minutes == 1 { "" } else { "s" }
        ));
    }
    parts.join(" ")
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

pub(crate) fn rest_default_minutes(
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
    use super::*;
    use crate::spacetimedb::*;

    #[test]
    fn rest_pages_keep_a_gettable_refresh_marker_across_repeated_refreshes() {
        let summary = RestSummary {
            minutes: 480,
            full_board_gold_spent: 1,
            additional_gold_spent: 0,
            gold_earned: 0,
            notoriety_gained: 0.0,
            healed: Vec::new(),
            trained: Vec::new(),
        };
        for (location, kind, expected) in [
            ("Inn", "inn", "/settlements/riverdale/inn"),
            ("Church", "temple", "/settlements/riverdale/religion"),
        ] {
            for rest_summary in [Some(&summary), None] {
                let markup = rest_service_menu(
                    location,
                    "riverdale",
                    kind,
                    None,
                    rest_summary,
                    SoapRestPreview::default(),
                )
                .into_string();
                assert!(markup.contains(&format!("data-live-refresh-url=\"{expected}\"")));
                assert!(!markup.contains("data-live-refresh-url=\"/settlements/riverdale/rest/"));
            }
        }
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
    fn settlement_wake_control_is_accessible_and_defaults_to_eight() {
        let markup = settlement_rest_duration_control(1_440, "hours").into_string();
        assert!(markup.contains("data-wake-time"));
        assert!(markup.contains("type=\"range\""));
        assert!(markup.contains("step=\"60\""));
        assert!(markup.contains("value=\"480\""));
        assert!(markup.contains("type=\"text\""));
        assert!(markup.contains("value=\"24:00\""));
        assert!(markup.contains("pattern=\"[0-9]+:[0-5][0-9]\""));
        assert!(markup.contains("aria-label=\"Wake time\""));
        assert!(markup.contains("aria-valuetext=\"08:00\""));
        assert!(markup.contains("name=\"requested_minutes\""));
    }

    #[test]
    fn rest_supplies_are_icons_with_hover_only_details() {
        let markup = soap_wash_preview(SoapRestPreview {
            total_units: 1,
            personal_units: 1,
            available_units: 1,
            alcohol_available: true,
            alcohol_will_be_consumed: false,
            ..SoapRestPreview::default()
        })
        .into_string();
        assert!(markup.contains("aria-label=\"Soap\""));
        assert!(markup.contains("aria-label=\"Alcohol\""));
        assert!(markup.contains("water-drop.svg"));
        assert!(markup.contains("beer-stein.svg"));
        assert!(markup.contains("rest-consumable-indicator available"));
        assert!(markup.contains("rest-consumable-indicator unavailable"));
        assert!(!markup.contains("rest-soap-preview"));
        assert!(markup.contains("Temperate characters do not drink"));
    }

    #[test]
    fn days_recommendation_keeps_slider_disabled_and_minimum_one() {
        let markup = settlement_rest_duration_control(3 * 1_440, "days").into_string();
        assert!(markup.contains("value=\"days\" checked"));
        assert!(markup.contains("aria-disabled=\"true\""));
        assert!(
            markup.contains(
                "value=\"480\" aria-label=\"Wake time\" aria-valuetext=\"08:00\" disabled"
            )
        );
        assert!(markup.contains("type=\"number\" name=\"duration\" value=\"3\""));
        assert!(markup.contains("min=\"1\" max=\"365\" step=\"1\""));
        assert!(markup.contains("name=\"requested_minutes\" disabled"));
    }

    #[test]
    fn rest_summary_duration_keeps_subday_hours_and_minutes() {
        assert_eq!(format_rest_duration(1_441), "1 day 1 minute");
        assert_eq!(format_rest_duration(1_920), "1 day 8 hours");
        assert_eq!(format_rest_duration(2_879), "1 day 23 hours 59 minutes");
    }
}
