//! Strategic schedule activities which combine training with other outcomes.

use crate::strategic_time::MINUTES_PER_DAY;

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
    (hours.max(0.0) * (strength_check.max(0.0) + endurance_check.max(0.0)) / 8.0).round() as u32
}

pub fn thievery_gold(hours: f32, population_scale: f32, stealth_check: f32) -> u32 {
    (hours.max(0.0) * population_scale.max(0.0) * (1.0 + stealth_check.max(0.0)) / 8.0).round()
        as u32
}

pub fn thievery_notoriety(hours: f32, population_scale: f32, stealth_check: f32) -> f32 {
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

pub fn raiding_notoriety(hours: f32) -> f32 {
    hours.max(0.0) * 1.5
}

pub fn raiding_retaliation_chance(hours: f32) -> f32 {
    1.0 - (-0.35 * hours.max(0.0)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn stealth_improves_thievery_outcomes() {
        assert!(thievery_gold(8.0, 2.0, 4.0) > thievery_gold(8.0, 2.0, 1.0));
        assert!(thievery_notoriety(8.0, 2.0, 4.0) < thievery_notoriety(8.0, 2.0, 1.0));
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
        assert_eq!(raiding_notoriety(8.0), 12.0);
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
