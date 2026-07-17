//! Pure calculations used by the strategic clock, training, and recovery systems.

pub const MINUTES_PER_DAY: u64 = 24 * 60;
pub const MINUTES_PER_YEAR: u64 = 365 * MINUTES_PER_DAY;
/// Natural recovery while taking full settlement downtime.
pub const HEALTH_RECOVERED_PER_DAY: f32 = 0.05;

/// Convert real elapsed time to authoritative strategic minutes.
pub fn elapsed_official_minutes(epoch_micros: i64, now_micros: i64) -> u64 {
    let elapsed_micros = now_micros.saturating_sub(epoch_micros).max(0) as u128;
    // One real week per 365-day game year: 84/73 seconds per game minute.
    (elapsed_micros.saturating_mul(73) / 84_000_000) as u64
}

/// Sum the daily minutes assigned to training and labor activities.
pub fn allocated_schedule_minutes(daily_minutes: [u16; 12]) -> u64 {
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
}
