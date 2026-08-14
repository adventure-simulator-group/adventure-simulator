//! Strategic schedule activities which combine training with other outcomes.

use crate::strategic_time::MINUTES_PER_DAY;

pub const THIEVERY_UNAVAILABLE_REASON: &str = "Thievery is only available inside settlements.";
pub const RAIDING_UNAVAILABLE_REASON: &str =
    "Raiding is only available at an eligible outdoor location.";
pub const CAROUSING_UNAVAILABLE_REASON: &str = "Carousing requires a settlement with an inn.";

/// The location facts which affect ordinary downtime activities. This is kept
/// independent of persistence and transport types so both authoritative
/// execution and observer-facing rendering use the same policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityLocation {
    Settlement { has_inn: bool },
    NamedOutdoorLocation,
    IneligibleNamedLocation,
    JourneyCamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationActivity {
    Carousing,
    Thievery,
    Raiding,
}

pub const ACTIVITY_SEGMENT_MINUTES: u16 = 15;

/// Replace unavailable planned activities one quarter-hour at a time.
///
/// Each replacement draw is weighted by the original planned minutes of the
/// available, non-Leisure activities. The seed makes authoritative execution
/// and client previews agree without persisting a second schedule. When no
/// planned activity is available, the removed time remains unallocated and is
/// therefore Leisure.
pub fn redistribute_unavailable_segments<const N: usize>(
    allocations: [u16; N],
    available: [bool; N],
    seed: u64,
) -> [u16; N] {
    let mut effective = allocations;
    let mut unavailable_segments = 0_u64;
    for index in 0..N {
        if !available[index] {
            unavailable_segments = unavailable_segments
                .saturating_add(u64::from(effective[index] / ACTIVITY_SEGMENT_MINUTES));
            effective[index] = 0;
        }
    }

    let total_weight = allocations
        .iter()
        .zip(available)
        .filter_map(|(minutes, is_available)| is_available.then_some(u64::from(*minutes)))
        .sum::<u64>();
    if total_weight == 0 {
        return effective;
    }

    for segment in 0..unavailable_segments {
        let mut draw = redistribution_roll(seed, segment) % total_weight;
        for index in 0..N {
            let weight = if available[index] {
                u64::from(allocations[index])
            } else {
                0
            };
            if draw < weight {
                effective[index] = effective[index].saturating_add(ACTIVITY_SEGMENT_MINUTES);
                break;
            }
            draw = draw.saturating_sub(weight);
        }
    }
    effective
}

fn redistribution_roll(seed: u64, segment: u64) -> u64 {
    let mut value = seed ^ 0xA4C7_1D5B_93E2_F860 ^ segment.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

impl ActivityLocation {
    pub const fn allows(self, activity: LocationActivity) -> bool {
        matches!(
            (self, activity),
            (
                Self::Settlement { has_inn: true },
                LocationActivity::Carousing
            ) | (Self::Settlement { .. }, LocationActivity::Thievery)
                | (Self::NamedOutdoorLocation, LocationActivity::Raiding)
        )
    }

    pub const fn unavailable_reason(self, activity: LocationActivity) -> Option<&'static str> {
        if self.allows(activity) {
            return None;
        }
        Some(match activity {
            LocationActivity::Carousing => CAROUSING_UNAVAILABLE_REASON,
            LocationActivity::Thievery => THIEVERY_UNAVAILABLE_REASON,
            LocationActivity::Raiding => RAIDING_UNAVAILABLE_REASON,
        })
    }
}

pub const ACTIVITY_TRAINING_RATE: f32 = 0.25;
pub const PRAYER_MORALE_LIMIT: f32 = 4.0;
pub const PRAYER_MORALE_SCALE_MINUTES: f32 = 60.0;
pub const MAX_DAILY_PRAYER_OBLIGATION_MINUTES: f32 = 120.0;
pub const DAYS_PER_WEEK: u64 = 7;
pub const SUNDAY_INDEX: u64 = 6;
pub const CAROUSING_MORALE_LIMIT: f32 = 4.0;
pub const CAROUSING_MORALE_SCALE_MINUTES: f32 = 120.0;

pub fn carousing_morale_per_day(minutes: u16) -> f32 {
    CAROUSING_MORALE_LIMIT * (1.0 - (-f32::from(minutes) / CAROUSING_MORALE_SCALE_MINUTES).exp())
}

pub fn is_sunday(day: u64) -> bool {
    day % DAYS_PER_WEEK == SUNDAY_INDEX
}

pub fn sundays_overlapping(start_minute: u64, elapsed_minutes: u64) -> Vec<u64> {
    if elapsed_minutes == 0 {
        return Vec::new();
    }
    let first_day = start_minute / MINUTES_PER_DAY;
    let last_day = start_minute
        .saturating_add(elapsed_minutes)
        .saturating_sub(1)
        / MINUTES_PER_DAY;
    (first_day..=last_day)
        .filter(|day| is_sunday(*day))
        .collect()
}

pub fn prayer_morale(prayer_minutes: u16) -> f32 {
    PRAYER_MORALE_LIMIT * (1.0 - (-f32::from(prayer_minutes) / PRAYER_MORALE_SCALE_MINUTES).exp())
}

pub fn led_prayer_morale(prayer_minutes: u16, religion_check: f32) -> f32 {
    prayer_morale(prayer_minutes) * religion_check.clamp(0.0, 5.0) / 5.0
}

pub fn meditation_morale(minutes: u16) -> f32 {
    prayer_morale(minutes) * 0.25
}

pub fn prayer_observance(fervor: f32, prayer_minutes: u16) -> f32 {
    let required = MAX_DAILY_PRAYER_OBLIGATION_MINUTES * fervor.clamp(0.0, 1.0);
    if required <= 0.0 {
        1.0
    } else {
        (f32::from(prayer_minutes) / required).clamp(0.0, 1.0)
    }
}

pub fn settlement_population_scale(population_level: i32, population_estimate: u32) -> f32 {
    if population_estimate > 0 {
        ((population_estimate as f32 + 1.0).ln() / 4.0).clamp(1.0, 4.0)
    } else {
        (population_level.max(1) as f32 / 2.0).clamp(0.5, 3.0)
    }
}

pub fn labor_gold(hours: f32, strength_check: f32, endurance_check: f32) -> u32 {
    (hours.max(0.0) * (strength_check.max(0.0) + endurance_check.max(0.0)) / 4.0).round() as u32
}

pub fn thievery_gold(hours: f32, population_scale: f32, stealth_check: f32) -> u32 {
    (hours.max(0.0) * population_scale.max(0.0) * (1.0 + stealth_check.max(0.0)) / 8.0).round()
        as u32
}

pub fn thievery_infamy(hours: f32, population_scale: f32, stealth_check: f32) -> f32 {
    hours.max(0.0) * population_scale.max(0.0) * 0.5 / (1.0 + stealth_check.max(0.0))
}

pub fn thievery_discovery_chance(hours: f32, population_scale: f32, stealth_check: f32) -> f32 {
    let exposure =
        0.12 * hours.max(0.0) * population_scale.max(0.0) / (1.0 + stealth_check.max(0.0));
    1.0 - (-exposure).exp()
}

pub fn raiding_gold(hours: f32, combat_check: f32) -> u32 {
    (hours.max(0.0) * (2.0 + combat_check.max(0.0)) / 6.0).round() as u32
}

pub fn raiding_infamy(hours: f32) -> f32 {
    hours.max(0.0) * 1.5
}

pub fn raiding_retaliation_chance(hours: f32) -> f32 {
    1.0 - (-0.35 * hours.max(0.0)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_activity_matrix_distinguishes_inns_settlements_and_outdoors() {
        let inn = ActivityLocation::Settlement { has_inn: true };
        let no_inn = ActivityLocation::Settlement { has_inn: false };
        let outdoors = ActivityLocation::NamedOutdoorLocation;
        let ineligible = ActivityLocation::IneligibleNamedLocation;
        let camp = ActivityLocation::JourneyCamp;

        assert!(inn.allows(LocationActivity::Thievery));
        assert!(inn.allows(LocationActivity::Carousing));
        assert!(!inn.allows(LocationActivity::Raiding));
        assert!(no_inn.allows(LocationActivity::Thievery));
        assert!(!no_inn.allows(LocationActivity::Carousing));
        assert!(outdoors.allows(LocationActivity::Raiding));
        assert!(!outdoors.allows(LocationActivity::Thievery));
        assert!(!outdoors.allows(LocationActivity::Carousing));
        assert!(!ineligible.allows(LocationActivity::Raiding));
        assert!(!camp.allows(LocationActivity::Raiding));
    }

    #[test]
    fn unavailable_segments_are_weighted_by_existing_available_allocations() {
        let allocations = [60, 120, 90];
        let available = [true, true, false];
        let mut first = 0_u64;
        let mut second = 0_u64;
        for seed in 0..4_000 {
            let effective = redistribute_unavailable_segments(allocations, available, seed);
            first += u64::from(effective[0] - allocations[0]);
            second += u64::from(effective[1] - allocations[1]);
            assert_eq!(
                effective
                    .iter()
                    .map(|minutes| u64::from(*minutes))
                    .sum::<u64>(),
                270
            );
        }
        let ratio = second as f64 / first as f64;
        assert!((1.9..2.1).contains(&ratio), "observed ratio {ratio}");
    }

    #[test]
    fn unavailable_segments_become_leisure_without_an_available_plan() {
        assert_eq!(
            redistribute_unavailable_segments([60, 120], [false, false], 7),
            [0, 0]
        );
    }

    #[test]
    fn redistribution_is_seeded_and_uses_quarter_hour_segments() {
        let allocations = [60, 120, 90];
        let available = [true, true, false];
        let first = redistribute_unavailable_segments(allocations, available, 42);
        assert_eq!(first, [75, 195, 0]);
        assert_eq!(
            first,
            redistribute_unavailable_segments(allocations, available, 42)
        );
        assert!(
            first
                .iter()
                .all(|minutes| minutes % ACTIVITY_SEGMENT_MINUTES == 0)
        );
        assert_eq!(
            first.iter().map(|minutes| u64::from(*minutes)).sum::<u64>(),
            270
        );
    }

    #[test]
    fn prayer_has_saturating_morale_and_fervor_scaled_observance() {
        assert_eq!(prayer_morale(0), 0.0);
        assert!(prayer_morale(120) > prayer_morale(60));
        assert!(prayer_morale(120) < PRAYER_MORALE_LIMIT);
        assert_eq!(prayer_observance(0.5, 30), 0.5);
        assert_eq!(prayer_observance(0.5, 60), 1.0);
        assert_eq!(led_prayer_morale(60, 0.0), 0.0);
        assert_eq!(led_prayer_morale(60, 5.0), prayer_morale(60));
        assert_eq!(meditation_morale(60), prayer_morale(60) * 0.25);
    }

    #[test]
    fn average_labor_covers_retail_daily_meals_and_inn_full_board() {
        let meal = crate::food::definition("cooked_meal").expect("standard cooked meal");
        let meals_per_day =
            (crate::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY / meal.kcal_per_unit).ceil() as u32;
        let retail_meal = crate::strategic_economy::language_adjusted_buy_price(
            crate::strategic_economy::merchant_buy_price(meal.value_per_unit.ceil() as u32),
            0.0,
        );
        let retail_daily_meals = meals_per_day.saturating_mul(retail_meal);
        let daily_labor = labor_gold(8.0, 2.0, 2.0);

        assert_eq!(daily_labor, 8);
        assert_eq!(retail_daily_meals, 6);
        assert!(daily_labor >= retail_daily_meals);
        assert!(retail_daily_meals >= crate::strategic_economy::INN_FULL_BOARD_GOLD_PER_DAY);
    }

    #[test]
    fn stealth_improves_thievery_outcomes() {
        assert!(thievery_gold(8.0, 2.0, 4.0) > thievery_gold(8.0, 2.0, 1.0));
        assert!(thievery_infamy(8.0, 2.0, 4.0) < thievery_infamy(8.0, 2.0, 1.0));
        assert!(
            thievery_discovery_chance(8.0, 2.0, 4.0) < thievery_discovery_chance(8.0, 2.0, 1.0)
        );
    }

    #[test]
    fn settlement_population_scale_uses_estimates_when_available() {
        assert_eq!(settlement_population_scale(1, 0), 0.5);
        assert!(settlement_population_scale(1, 10_000) > 2.0);
        assert_eq!(settlement_population_scale(10, 0), 3.0);
    }

    #[test]
    fn raiding_is_conspicuous() {
        assert!(raiding_retaliation_chance(8.0) > 0.9);
        assert_eq!(raiding_infamy(8.0), 12.0);
    }

    #[test]
    fn sunday_is_every_seventh_calendar_day() {
        assert!(!is_sunday(0));
        assert!(!is_sunday(5));
        assert!(is_sunday(6));
        assert!(is_sunday(13));
    }

    #[test]
    fn travel_detects_each_sunday_it_overlaps() {
        let saturday_evening = 5 * MINUTES_PER_DAY + 20 * 60;
        assert_eq!(sundays_overlapping(saturday_evening, 32 * 60), vec![6]);
        assert_eq!(
            sundays_overlapping(saturday_evening, 8 * MINUTES_PER_DAY),
            vec![6, 13]
        );
        assert!(sundays_overlapping(0, 0).is_empty());
    }
}
