//! Pure calculations used by the strategic clock, training, and recovery systems.

use crate::{
    provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY,
    strategic_schedule::{DailySchedule, settlement_leisure_outcome},
};

pub const MINUTES_PER_DAY: u64 = 24 * 60;
pub const MINUTES_PER_YEAR: u64 = 365 * MINUTES_PER_DAY;
pub const DEFAULT_WALKING_MINUTES_PER_DAY: u16 = 8 * 60;
pub const MIN_WALKING_MINUTES_PER_DAY: u16 = 60;
pub const MAX_WALKING_MINUTES_PER_DAY: u16 = 16 * 60;
pub const LUNAR_CYCLE_MINUTES: u64 = 42_524;
pub const MAX_ITINERARY_SEGMENTS: usize = 512;
/// Natural recovery while taking full settlement downtime.
pub const HEALTH_RECOVERED_PER_DAY: f32 = 0.05;

/// The fatigue inputs that determine one party member's available marching
/// time.  Keeping this small and data-only lets both the strategic reducer and
/// the HTML travel preview use exactly the same calculation.
#[derive(Clone, Copy, Debug, PartialEq)]
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

fn next_walking_start(absolute_minute: u64, walking_minutes: u16) -> Option<u64> {
    let (start, end) = daylight_walking_window(walking_minutes)?;
    let day = absolute_minute / MINUTES_PER_DAY;
    let minute = (absolute_minute % MINUTES_PER_DAY) as u16;
    if minute < start {
        Some(day * MINUTES_PER_DAY + u64::from(start))
    } else if minute < end {
        Some(absolute_minute)
    } else {
        Some((day + 1) * MINUTES_PER_DAY + u64::from(start))
    }
}

pub fn forecast_itinerary(
    start_minute: u64,
    movement_minutes: u64,
    walking_minutes_per_day: u16,
    camp_policy: CampDurationPolicy,
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
    // A leader-selected camp policy applies once after departure/current
    // fatigue and once after each walking window. If that duration overshoots
    // the next window, the following segment is only the daylight wait; this
    // prevents a long fixed camp from repeatedly skipping every walk window.
    let mut policy_due = true;
    while movement < movement_minutes {
        if segments.len() >= MAX_ITINERARY_SEGMENTS {
            truncated = true;
            break;
        }
        let walk_start = next_walking_start(absolute, walking_minutes_per_day)?;
        if walk_start > absolute {
            let wait = walk_start - absolute;
            let required = common_fatigue_clear_minutes(&members);
            let duration = match (policy_due, camp_policy) {
                (true, CampDurationPolicy::Auto) => wait.max(required),
                (true, CampDurationPolicy::FixedMinutes(value)) => wait.max(u64::from(value)),
                (false, _) => wait,
            };
            let (average_start, _) = fatigue_summary(&members);
            for member in &mut members {
                member.calories_used =
                    camp_fatigue_after(member.calories_used, duration, member.camp_schedule);
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
            policy_due = false;
            continue;
        }
        let (_, window_end) = daylight_walking_window(walking_minutes_per_day)?;
        let day_start = absolute / MINUTES_PER_DAY * MINUTES_PER_DAY;
        let available = day_start
            .saturating_add(u64::from(window_end))
            .saturating_sub(absolute);
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
        policy_due = true;
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
    fn walking_hours_are_bounded_and_centered_on_noon() {
        assert_eq!(daylight_walking_window(8 * 60), Some((8 * 60, 16 * 60)));
        assert_eq!(daylight_walking_window(7 * 60 + 1), Some((510, 931)));
        assert_eq!(daylight_walking_window(59), None);
        assert_eq!(daylight_walking_window(16 * 60 + 1), None);
    }

    #[test]
    fn itinerary_tracks_elapsed_separately_from_movement() {
        let forecast = forecast_itinerary(
            8 * 60,
            12 * 60,
            8 * 60,
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
            CampDurationPolicy::Auto,
            &[itinerary_member(0.0, 6_000.0)],
        )
        .unwrap();
        assert_eq!(forecast.total_movement_minutes, outbound * 2);
        assert!(!forecast.truncated);
    }

    #[test]
    fn long_fixed_camp_cannot_skip_every_future_walking_window() {
        let forecast = forecast_itinerary(
            8 * 60,
            10 * 60,
            8 * 60,
            CampDurationPolicy::FixedMinutes(20 * 60),
            &[itinerary_member(0.0, 6_000.0)],
        )
        .unwrap();
        assert_eq!(forecast.total_movement_minutes, 10 * 60);
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
            CampDurationPolicy::Auto,
            &[itinerary_member(0.0, 6_000.0)],
        )
        .unwrap();
        assert!(forecast.truncated);
        assert_eq!(forecast.segments.len(), MAX_ITINERARY_SEGMENTS);
    }
}
