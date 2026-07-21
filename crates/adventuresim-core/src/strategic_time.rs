//! Pure calculations used by the strategic clock, training, and recovery systems.

use crate::{
    provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY,
    strategic_schedule::{DailySchedule, settlement_leisure_outcome},
};

pub const MINUTES_PER_DAY: u64 = 24 * 60;
pub const MINUTES_PER_YEAR: u64 = 365 * MINUTES_PER_DAY;
pub const DEFAULT_WALKING_MINUTES_PER_DAY: u16 = 8 * 60;
pub const MIN_WALKING_MINUTES_PER_DAY: u16 = 1;
pub const MAX_WALKING_MINUTES_PER_DAY: u16 = 24 * 60;
pub const LUNAR_CYCLE_MINUTES: u64 = 42_524;
pub const MAX_ITINERARY_SEGMENTS: usize = 512;
/// Natural recovery while taking full settlement downtime.
pub const HEALTH_RECOVERED_PER_DAY: f32 = 0.05;

/// The fatigue inputs that determine one party member's available marching
/// time.  Keeping this small and data-only lets both the strategic reducer and
/// the HTML travel preview use exactly the same calculation.
#[derive(Clone, Debug, PartialEq)]
pub struct TravelFatigueInputs {
    pub fatigue_capacity: f32,
    pub calories_used: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CampDurationPolicy {
    #[default]
    Auto,
    FixedMinutes(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItineraryMember {
    pub fatigue_capacity: f32,
    pub calories_used: f32,
    pub camp_schedule: DailySchedule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItinerarySegmentKind {
    Walking,
    Camp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItinerarySegment {
    pub kind: ItinerarySegmentKind,
    pub elapsed_start: u64,
    pub elapsed_minutes: u64,
    pub movement_start: u64,
    pub movement_minutes: u64,
    pub average_fatigue_start: f32,
    pub average_fatigue_end: f32,
    pub maximum_fatigue_end: f32,
    pub required_rest_minutes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItineraryForecast {
    pub segments: Vec<ItinerarySegment>,
    pub total_elapsed_minutes: u64,
    pub total_movement_minutes: u64,
    pub truncated: bool,
}

fn fatigue_fraction(member: &ItineraryMember) -> f32 {
    member.calories_used.max(0.0) / member.fatigue_capacity.max(0.01)
}

fn fatigue_summary(members: &[ItineraryMember]) -> (f32, f32) {
    if members.is_empty() {
        return (0.0, 0.0);
    }
    let fractions = members.iter().map(fatigue_fraction);
    let total = fractions.clone().sum::<f32>();
    let maximum = fractions.fold(0.0, f32::max);
    (total / members.len() as f32, maximum)
}

pub fn minutes_until_fatigue_clears(calories_used: f32) -> u64 {
    (calories_used.max(0.0) / STRATEGIC_TRAVEL_KCAL_PER_DAY * MINUTES_PER_DAY as f32).ceil() as u64
}

pub fn common_fatigue_clear_minutes(members: &[ItineraryMember]) -> u64 {
    members
        .iter()
        .map(|member| minutes_until_fatigue_clears(member.calories_used))
        .max()
        .unwrap_or(0)
}

pub fn camp_fatigue_after(
    calories_used: f32,
    elapsed_minutes: u64,
    schedule: DailySchedule,
) -> f32 {
    let rest_minutes = minutes_until_fatigue_clears(calories_used).min(elapsed_minutes);
    let remaining = elapsed_minutes.saturating_sub(rest_minutes);
    if remaining == 0 {
        return (calories_used
            - STRATEGIC_TRAVEL_KCAL_PER_DAY * rest_minutes as f32 / MINUTES_PER_DAY as f32)
            .max(0.0);
    }
    settlement_leisure_outcome(schedule, remaining, 0.0)
        .fatigue_delta
        .max(0.0)
}

pub fn daylight_walking_window(walking_minutes: u16) -> Option<(u16, u16)> {
    if !(MIN_WALKING_MINUTES_PER_DAY..=MAX_WALKING_MINUTES_PER_DAY).contains(&walking_minutes) {
        return None;
    }
    let start = 12 * 60 - walking_minutes / 2;
    Some((start, start + walking_minutes))
}

fn walking_window_at_or_after(
    absolute_minute: u64,
    walking_minutes: u16,
    travel_at_night: bool,
) -> Option<(u64, u64)> {
    let (start, end) =
        scheduled_walking_window_at_or_after(absolute_minute, walking_minutes, travel_at_night)?;
    Some((start.max(absolute_minute), end))
}

/// Return whether the supplied canonical minute falls inside the party's
/// configured daily walking window.
pub fn is_walking_time(absolute_minute: u64, walking_minutes: u16, travel_at_night: bool) -> bool {
    let Some((start, end)) =
        scheduled_walking_window_at_or_after(absolute_minute, walking_minutes, travel_at_night)
    else {
        return false;
    };
    absolute_minute >= start && absolute_minute < end
}

/// Return the minutes until the next scheduled start of the walking window.
/// When the party is already inside today's window, this deliberately points
/// at the following day's start so an optional extra rest still wakes on the
/// established daily schedule.
pub fn minutes_until_next_walking_start(
    absolute_minute: u64,
    walking_minutes: u16,
    travel_at_night: bool,
) -> Option<u64> {
    let (start, end) =
        scheduled_walking_window_at_or_after(absolute_minute, walking_minutes, travel_at_night)?;
    let next_start = if absolute_minute < start {
        start
    } else {
        scheduled_walking_window_at_or_after(end, walking_minutes, travel_at_night)?.0
    };
    Some(next_start.saturating_sub(absolute_minute))
}

fn scheduled_walking_window_at_or_after(
    absolute_minute: u64,
    walking_minutes: u16,
    travel_at_night: bool,
) -> Option<(u64, u64)> {
    let (day_start, day_end) = daylight_walking_window(walking_minutes)?;
    let day = absolute_minute / MINUTES_PER_DAY;
    if !travel_at_night {
        let start = day * MINUTES_PER_DAY + u64::from(day_start);
        let end = day * MINUTES_PER_DAY + u64::from(day_end);
        return if absolute_minute < end {
            Some((start, end))
        } else {
            Some((start + MINUTES_PER_DAY, end + MINUTES_PER_DAY))
        };
    }

    let before_midnight = u64::from(walking_minutes / 2);
    let after_midnight = u64::from(walking_minutes) - before_midnight;
    let current_midnight = day * MINUTES_PER_DAY;
    let current_start = current_midnight.saturating_sub(before_midnight);
    let current_end = current_midnight.saturating_add(after_midnight);
    if absolute_minute < current_end {
        return Some((current_start, current_end));
    }
    let next_midnight = current_midnight.saturating_add(MINUTES_PER_DAY);
    Some((
        next_midnight.saturating_sub(before_midnight),
        next_midnight.saturating_add(after_midnight),
    ))
}

pub fn forecast_itinerary(
    start_minute: u64,
    movement_minutes: u64,
    walking_minutes_per_day: u16,
    travel_at_night: bool,
    _camp_policy: CampDurationPolicy,
    members: &[ItineraryMember],
) -> Option<ItineraryForecast> {
    daylight_walking_window(walking_minutes_per_day)?;
    if members.is_empty() {
        return None;
    }
    let mut members = members.to_vec();
    let mut segments = Vec::new();
    let mut movement = 0_u64;
    let mut absolute = start_minute;
    let mut truncated = false;
    while movement < movement_minutes {
        if segments.len() >= MAX_ITINERARY_SEGMENTS {
            truncated = true;
            break;
        }
        let (walk_start, walk_end) =
            walking_window_at_or_after(absolute, walking_minutes_per_day, travel_at_night)?;
        if walk_start > absolute {
            // Every minute outside the daily walking window is camp/downtime.
            // A complete post-walk interval is therefore exactly
            // 24 hours minus the configured walking time; the first interval
            // may be shorter when a journey begins partway through the day.
            let duration = walk_start - absolute;
            let required = common_fatigue_clear_minutes(&members);
            let (average_start, _) = fatigue_summary(&members);
            for member in &mut members {
                member.calories_used = camp_fatigue_after(
                    member.calories_used,
                    duration,
                    member.camp_schedule.clone(),
                );
            }
            let (average_end, maximum_end) = fatigue_summary(&members);
            segments.push(ItinerarySegment {
                kind: ItinerarySegmentKind::Camp,
                elapsed_start: absolute.saturating_sub(start_minute),
                elapsed_minutes: duration,
                movement_start: movement,
                movement_minutes: 0,
                average_fatigue_start: average_start,
                average_fatigue_end: average_end,
                maximum_fatigue_end: maximum_end,
                required_rest_minutes: required,
            });
            absolute = absolute.saturating_add(duration);
            continue;
        }
        let available = walk_end.saturating_sub(absolute);
        let duration = available.min(movement_minutes.saturating_sub(movement));
        if duration == 0 {
            absolute = absolute.saturating_add(1);
            continue;
        }
        let (average_start, _) = fatigue_summary(&members);
        for member in &mut members {
            member.calories_used +=
                STRATEGIC_TRAVEL_KCAL_PER_DAY * duration as f32 / MINUTES_PER_DAY as f32;
        }
        movement = movement.saturating_add(duration);
        let (average_end, maximum_end) = fatigue_summary(&members);
        segments.push(ItinerarySegment {
            kind: ItinerarySegmentKind::Walking,
            elapsed_start: absolute.saturating_sub(start_minute),
            elapsed_minutes: duration,
            movement_start: movement.saturating_sub(duration),
            movement_minutes: duration,
            average_fatigue_start: average_start,
            average_fatigue_end: average_end,
            maximum_fatigue_end: maximum_end,
            required_rest_minutes: 0,
        });
        absolute = absolute.saturating_add(duration);
    }
    Some(ItineraryForecast {
        segments,
        total_elapsed_minutes: absolute.saturating_sub(start_minute),
        total_movement_minutes: movement,
        truncated,
    })
}

/// Canonical lunar cycle fraction: 0=new, .25=first quarter, .5=full,
/// .75=last quarter. Day 1 00:00 is a new moon.
pub fn lunar_phase(absolute_minute: u64) -> f64 {
    (absolute_minute % LUNAR_CYCLE_MINUTES) as f64 / LUNAR_CYCLE_MINUTES as f64
}

pub fn lunar_illumination(phase: f64) -> f64 {
    (1.0 - (std::f64::consts::TAU * phase.rem_euclid(1.0)).cos()) / 2.0
}

/// Return the next leg's length for the least-rested party member. A one-minute
/// minimum lets an already-tired party establish camp rather than becoming
/// stranded. `None` represents a party with no members.
pub fn party_travel_leg_minutes(
    members: &[TravelFatigueInputs],
    fatigue_percent: u8,
) -> Option<u64> {
    let threshold = f32::from(fatigue_percent) / 100.0;
    members
        .iter()
        .map(|member| {
            let remaining_calories =
                (threshold * member.fatigue_capacity - member.calories_used).max(0.0);
            (remaining_calories / STRATEGIC_TRAVEL_KCAL_PER_DAY * MINUTES_PER_DAY as f32).ceil()
                as u64
        })
        .map(|minutes| minutes.max(1))
        .min()
}

/// Convert real elapsed time to authoritative strategic minutes.
pub fn elapsed_official_minutes(epoch_micros: i64, now_micros: i64) -> u64 {
    let elapsed_micros = now_micros.saturating_sub(epoch_micros).max(0) as u128;
    // One real week per 365-day game year: 84/73 seconds per game minute.
    (elapsed_micros.saturating_mul(73) / 84_000_000) as u64
}

/// Sum the daily minutes assigned to training and labor activities.
pub fn allocated_schedule_minutes<const N: usize>(daily_minutes: [u16; N]) -> u64 {
    daily_minutes.into_iter().map(u64::from).sum()
}

/// Calculate the training hours earned from one daily minute allocation.
pub fn training_hours_increment(elapsed_minutes: u64, daily_minutes: u16) -> f32 {
    let hours_per_day = f32::from(daily_minutes) / 60.0;
    let days = elapsed_minutes as f32 / MINUTES_PER_DAY as f32;
    days * hours_per_day
}

/// Apply natural recovery to one limb, capped at full health.
pub fn healed_health(health: f32, elapsed_minutes: u64) -> f32 {
    let recovery = elapsed_minutes as f32 / MINUTES_PER_DAY as f32 * HEALTH_RECOVERED_PER_DAY;
    (health + recovery).min(1.0)
}

/// Return the minutes needed for the least healthy limb to recover fully.
pub fn convalescence_minutes(limb_health: [f32; 7]) -> u64 {
    let lowest_health = limb_health.into_iter().fold(1.0_f32, f32::min);
    if lowest_health >= 1.0 {
        return 0;
    }

    if !lowest_health.is_finite() {
        return if lowest_health.is_sign_negative() {
            u64::MAX
        } else {
            0
        };
    }

    let estimate =
        ((1.0 - lowest_health) / HEALTH_RECOVERED_PER_DAY * MINUTES_PER_DAY as f32).ceil() as u64;
    let mut upper = estimate.max(1);
    for _ in 0..64 {
        if healed_health(lowest_health, upper) >= 1.0 {
            break;
        }
        let next = upper.saturating_mul(2);
        if next == upper {
            return u64::MAX;
        }
        upper = next;
    }
    if healed_health(lowest_health, upper) < 1.0 {
        return u64::MAX;
    }

    let mut lower = 0;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        if healed_health(lowest_health, middle) >= 1.0 {
            upper = middle;
        } else {
            lower = middle + 1;
        }
    }
    lower
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_real_week_is_one_game_year() {
        let one_week_micros = 7 * 24 * 60 * 60 * 1_000_000i64;
        assert_eq!(
            elapsed_official_minutes(0, one_week_micros),
            MINUTES_PER_YEAR
        );
    }

    #[test]
    fn future_epoch_has_no_elapsed_official_minutes() {
        assert_eq!(elapsed_official_minutes(2_000_000, 1_000_000), 0);
    }

    #[test]
    fn training_uses_the_daily_minute_allocation() {
        let schedule = [90, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 480];
        assert_eq!(
            training_hours_increment(MINUTES_PER_DAY * 2, schedule[0]),
            3.0
        );
        assert_eq!(
            training_hours_increment(MINUTES_PER_DAY * 2, schedule[1]),
            1.0
        );
        assert_eq!(allocated_schedule_minutes(schedule), 600);
    }

    #[test]
    fn convalescence_waits_for_the_slowest_limb() {
        let limb_health = [0.9, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let minutes = convalescence_minutes(limb_health);
        assert_eq!(minutes, MINUTES_PER_DAY * 2);
        assert_eq!(healed_health(0.9, minutes), 1.0);
        assert!(healed_health(0.9, minutes - 1) < 1.0);
    }

    #[test]
    fn healthy_limbs_need_no_convalescence() {
        assert_eq!(convalescence_minutes([1.0; 7]), 0);
    }

    #[test]
    fn healing_is_capped_at_full_health() {
        assert_eq!(healed_health(0.98, MINUTES_PER_DAY), 1.0);
    }

    #[test]
    fn travel_leg_uses_the_least_rested_member() {
        let members = [
            TravelFatigueInputs {
                fatigue_capacity: 6_000.0,
                calories_used: 0.0,
            },
            TravelFatigueInputs {
                fatigue_capacity: 6_000.0,
                calories_used: 1_500.0,
            },
        ];
        assert_eq!(party_travel_leg_minutes(&members, 50), Some(360));
    }

    #[test]
    fn exhausted_party_can_still_make_camp() {
        assert_eq!(
            party_travel_leg_minutes(
                &[TravelFatigueInputs {
                    fatigue_capacity: 6_000.0,
                    calories_used: 9_000.0,
                }],
                50,
            ),
            Some(1)
        );
    }

    fn itinerary_member(calories_used: f32, capacity: f32) -> ItineraryMember {
        ItineraryMember {
            fatigue_capacity: capacity,
            calories_used,
            camp_schedule: DailySchedule::default(),
        }
    }

    #[test]
    fn walking_hours_cover_the_full_day_slider_range() {
        assert_eq!(daylight_walking_window(8 * 60), Some((8 * 60, 16 * 60)));
        assert_eq!(daylight_walking_window(7 * 60 + 1), Some((510, 931)));
        assert_eq!(daylight_walking_window(15), Some((713, 728)));
        assert_eq!(daylight_walking_window(24 * 60), Some((0, 24 * 60)));
        assert_eq!(daylight_walking_window(0), None);
        assert_eq!(daylight_walking_window(24 * 60 + 1), None);
    }

    #[test]
    fn camp_wake_time_stays_on_the_absolute_daylight_schedule() {
        assert_eq!(
            minutes_until_next_walking_start(7 * 60, 8 * 60, false),
            Some(60)
        );
        assert!(!is_walking_time(7 * 60, 8 * 60, false));
        assert!(is_walking_time(9 * 60, 8 * 60, false));
        assert_eq!(
            minutes_until_next_walking_start(9 * 60, 8 * 60, false),
            Some(23 * 60)
        );
    }

    #[test]
    fn camp_wake_time_stays_on_the_absolute_night_schedule() {
        assert_eq!(
            minutes_until_next_walking_start(60, 8 * 60, true),
            Some(19 * 60)
        );
        assert_eq!(
            minutes_until_next_walking_start(18 * 60, 8 * 60, true),
            Some(2 * 60)
        );
        assert!(is_walking_time(21 * 60, 8 * 60, true));
        assert_eq!(
            minutes_until_next_walking_start(21 * 60, 8 * 60, true),
            Some(23 * 60)
        );
    }

    #[test]
    fn itinerary_tracks_elapsed_separately_from_movement() {
        let forecast = forecast_itinerary(
            8 * 60,
            12 * 60,
            8 * 60,
            false,
            CampDurationPolicy::Auto,
            &[itinerary_member(0.0, 6_000.0)],
        )
        .unwrap();
        assert_eq!(forecast.total_movement_minutes, 12 * 60);
        assert_eq!(forecast.total_elapsed_minutes, 28 * 60);
        assert_eq!(
            forecast
                .segments
                .iter()
                .map(|segment| segment.kind)
                .collect::<Vec<_>>(),
            [
                ItinerarySegmentKind::Walking,
                ItinerarySegmentKind::Camp,
                ItinerarySegmentKind::Walking
            ]
        );
    }

    #[test]
    fn partial_first_and_final_days_respect_daylight() {
        let forecast = forecast_itinerary(
            14 * 60,
            3 * 60,
            8 * 60,
            false,
            CampDurationPolicy::Auto,
            &[itinerary_member(0.0, 6_000.0)],
        )
        .unwrap();
        let walking: Vec<_> = forecast
            .segments
            .iter()
            .filter(|segment| segment.kind == ItinerarySegmentKind::Walking)
            .map(|segment| segment.elapsed_minutes)
            .collect();
        assert_eq!(walking, [2 * 60, 60]);
    }

    #[test]
    fn night_travel_is_one_window_centered_on_midnight() {
        let forecast = forecast_itinerary(
            20 * 60,
            10 * 60,
            8 * 60,
            true,
            CampDurationPolicy::Auto,
            &[itinerary_member(0.0, 6_000.0)],
        )
        .unwrap();
        assert_eq!(forecast.total_elapsed_minutes, 26 * 60);
        assert_eq!(
            forecast
                .segments
                .iter()
                .map(|segment| (segment.kind, segment.elapsed_minutes))
                .collect::<Vec<_>>(),
            [
                (ItinerarySegmentKind::Walking, 8 * 60),
                (ItinerarySegmentKind::Camp, 16 * 60),
                (ItinerarySegmentKind::Walking, 2 * 60),
            ]
        );
    }

    #[test]
    fn auto_and_fixed_camps_expose_retained_fatigue() {
        let members = [
            itinerary_member(3_000.0, 6_000.0),
            itinerary_member(1_500.0, 3_000.0),
        ];
        assert_eq!(common_fatigue_clear_minutes(&members), 12 * 60);
        assert_eq!(
            camp_fatigue_after(3_000.0, 6 * 60, DailySchedule::default()),
            1_500.0
        );
        assert_eq!(
            camp_fatigue_after(3_000.0, 12 * 60, DailySchedule::default()),
            0.0
        );
        let tiring = DailySchedule {
            labor: 24 * 60,
            ..Default::default()
        };
        assert!(camp_fatigue_after(0.0, MINUTES_PER_DAY, tiring) > 0.0);
    }

    #[test]
    fn quest_return_is_just_bounded_double_movement() {
        let outbound = 9 * 60;
        let forecast = forecast_itinerary(
            8 * 60,
            outbound * 2,
            8 * 60,
            false,
            CampDurationPolicy::Auto,
            &[itinerary_member(0.0, 6_000.0)],
        )
        .unwrap();
        assert_eq!(forecast.total_movement_minutes, outbound * 2);
        assert!(!forecast.truncated);
    }

    #[test]
    fn non_walking_time_is_implied_by_the_daily_walking_window() {
        let forecast = forecast_itinerary(
            8 * 60,
            10 * 60,
            8 * 60,
            false,
            CampDurationPolicy::FixedMinutes(20 * 60),
            &[itinerary_member(0.0, 6_000.0)],
        )
        .unwrap();
        assert_eq!(forecast.total_movement_minutes, 10 * 60);
        let camp = forecast
            .segments
            .iter()
            .find(|segment| segment.kind == ItinerarySegmentKind::Camp)
            .unwrap();
        assert_eq!(camp.elapsed_minutes, 16 * 60);
        assert!(!forecast.truncated);
    }

    #[test]
    fn lunar_phase_has_canonical_quarters_and_wraps_near_overflow() {
        assert_eq!(lunar_phase(0), 0.0);
        assert!((lunar_phase(LUNAR_CYCLE_MINUTES / 4) - 0.25).abs() < 0.0001);
        assert!((lunar_phase(LUNAR_CYCLE_MINUTES / 2) - 0.5).abs() < 0.0001);
        assert_eq!(lunar_phase(LUNAR_CYCLE_MINUTES), 0.0);
        assert!(lunar_phase(u64::MAX).is_finite());
        assert!((lunar_illumination(0.0) - 0.0).abs() < f64::EPSILON);
        assert!((lunar_illumination(0.5) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn itinerary_materialization_is_capped() {
        let forecast = forecast_itinerary(
            0,
            u64::MAX,
            MIN_WALKING_MINUTES_PER_DAY,
            false,
            CampDurationPolicy::Auto,
            &[itinerary_member(0.0, 6_000.0)],
        )
        .unwrap();
        assert!(forecast.truncated);
        assert_eq!(forecast.segments.len(), MAX_ITINERARY_SEGMENTS);
    }
}
