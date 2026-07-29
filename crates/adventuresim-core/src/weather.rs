//! Versioned, deterministic strategic weather.
//!
//! Weather is evaluated from authoritative absolute time and position.  It is
//! intentionally calculation-only: callers snapshot the result when a stable
//! journey or incident needs to outlive later rules changes.

use serde::{Deserialize, Serialize};

pub const WEATHER_RULES_VERSION: u16 = 1;
pub const WEATHER_INTERVAL_MINUTES: u64 = 360;
pub const WEATHER_CELL_MICRODEGREES: i32 = 250_000;
const HISTORY_INTERVALS: u64 = 16;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Precipitation {
    #[default]
    Clear,
    Rain,
    Snow,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WeatherSnapshot {
    pub rules_version: u16,
    pub interval_start_minute: u64,
    pub cell_latitude: i32,
    pub cell_longitude: i32,
    pub precipitation: Precipitation,
    /// Current precipitation intensity, in basis points.
    pub intensity_bps: u16,
    /// Antecedent liquid ground moisture, in basis points.
    pub ground_moisture_bps: u16,
    /// Established snow cover, in basis points.
    pub snow_cover_bps: u16,
}

impl WeatherSnapshot {
    pub const fn is_precipitating(self) -> bool {
        !matches!(self.precipitation, Precipitation::Clear)
    }

    pub const fn qualitative_ground(self) -> &'static str {
        if self.snow_cover_bps >= 6_000 {
            "Deep snow"
        } else if self.snow_cover_bps >= 1_500 {
            "Snow-covered"
        } else if self.ground_moisture_bps >= 7_000 {
            "Waterlogged"
        } else if self.ground_moisture_bps >= 3_000 {
            "Muddy"
        } else if self.ground_moisture_bps >= 800 {
            "Damp"
        } else {
            "Dry"
        }
    }
}

/// Evaluate the strategic weather authority.
///
/// Coordinates are signed decimal microdegrees and elevation is metres above
/// sea level. `world_seed` and the rules version domain-separate worlds and
/// later algorithms without storing per-cell simulation rows.
pub fn weather_at(
    world_seed: u64,
    absolute_minute: u64,
    latitude_microdegrees: i32,
    longitude_microdegrees: i32,
    elevation_m: i16,
) -> WeatherSnapshot {
    let cell_latitude = latitude_microdegrees.div_euclid(WEATHER_CELL_MICRODEGREES);
    let cell_longitude = longitude_microdegrees.div_euclid(WEATHER_CELL_MICRODEGREES);
    let interval = absolute_minute / WEATHER_INTERVAL_MINUTES;
    let current = interval_weather(
        world_seed,
        interval,
        cell_latitude,
        cell_longitude,
        elevation_m,
    );
    let mut moisture = 0u32;
    let mut snow = 0u32;
    let available_history = HISTORY_INTERVALS.min(interval.saturating_add(1));
    for age in (0..available_history).rev() {
        let sample = interval_weather(
            world_seed,
            interval.saturating_sub(age),
            cell_latitude,
            cell_longitude,
            elevation_m,
        );
        (moisture, snow) = advance_ground(
            moisture,
            snow,
            sample,
            temperature_deci_c(interval.saturating_sub(age), cell_latitude, elevation_m),
        );
    }
    WeatherSnapshot {
        rules_version: WEATHER_RULES_VERSION,
        interval_start_minute: interval * WEATHER_INTERVAL_MINUTES,
        cell_latitude,
        cell_longitude,
        precipitation: current.precipitation,
        intensity_bps: current.intensity_bps,
        ground_moisture_bps: moisture.min(10_000) as u16,
        snow_cover_bps: snow.min(10_000) as u16,
    }
}

fn advance_ground(
    mut moisture: u32,
    mut snow: u32,
    sample: IntervalWeather,
    temperature_deci_c: i32,
) -> (u32, u32) {
    // Liquid drains faster than established snow melts.
    moisture = moisture.saturating_mul(3) / 4;
    snow = snow.saturating_mul(7) / 8;
    match sample.precipitation {
        Precipitation::Rain => {
            moisture = moisture.saturating_add(u32::from(sample.intensity_bps) / 3);
            // Warm rain accelerates thaw.
            snow = snow.saturating_mul(3) / 4;
        }
        Precipitation::Snow => {
            snow = snow.saturating_add(u32::from(sample.intensity_bps) / 3);
        }
        Precipitation::Clear => {
            if temperature_deci_c > 20 {
                snow = snow.saturating_mul(7) / 8;
            }
        }
    }
    (moisture, snow)
}

#[derive(Clone, Copy)]
struct IntervalWeather {
    precipitation: Precipitation,
    intensity_bps: u16,
}

fn interval_weather(
    world_seed: u64,
    interval: u64,
    cell_latitude: i32,
    cell_longitude: i32,
    elevation_m: i16,
) -> IntervalWeather {
    let roll = weather_hash(world_seed, interval, cell_latitude, cell_longitude);
    let seasonal_wetness = seasonal_wetness_bps(interval);
    let precipitation_cutoff = 1_700u16.saturating_add(seasonal_wetness / 5);
    let occurrence = (roll % 10_000) as u16;
    if occurrence >= precipitation_cutoff {
        return IntervalWeather {
            precipitation: Precipitation::Clear,
            intensity_bps: 0,
        };
    }
    let intensity_bps = 1_000 + ((roll >> 16) % 9_001) as u16;
    let precipitation = if temperature_deci_c(interval, cell_latitude, elevation_m) <= 0 {
        Precipitation::Snow
    } else {
        Precipitation::Rain
    };
    IntervalWeather {
        precipitation,
        intensity_bps,
    }
}

fn seasonal_wetness_bps(interval: u64) -> u16 {
    let day = (interval * WEATHER_INTERVAL_MINUTES / 1_440) % 365;
    // Northern-European autumn/winter is wetter than summer.
    if !(90..270).contains(&day) {
        7_000
    } else {
        3_000
    }
}

fn temperature_deci_c(interval: u64, cell_latitude: i32, elevation_m: i16) -> i32 {
    let day = (interval * WEATHER_INTERVAL_MINUTES / 1_440) % 365;
    let seasonal = if day < 60 || day >= 330 {
        -40
    } else if day < 120 || day >= 270 {
        50
    } else {
        150
    };
    let latitude_penalty = (cell_latitude.abs().saturating_sub(160) * 2).min(180);
    let elevation_penalty = i32::from(elevation_m.max(0)) * 6 / 100;
    seasonal - latitude_penalty - elevation_penalty
}

fn weather_hash(seed: u64, interval: u64, lat: i32, lon: i32) -> u64 {
    let mut value = seed
        ^ (u64::from(WEATHER_RULES_VERSION) << 48)
        ^ interval.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (lat as i64 as u64).rotate_left(17)
        ^ (lon as i64 as u64).rotate_left(39);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// Supplement an underlying terrain check with the Snow overlay.
pub fn snow_overlay_check(underlying_bps: u16, snow_bps: u16, cover_bps: u16) -> u16 {
    let cover = u32::from(cover_bps.min(10_000));
    (((u32::from(underlying_bps) * (10_000 - cover)) + (u32::from(snow_bps) * cover)) / 10_000)
        as u16
}

/// Split travel practice without replacing the underlying biome exposure.
pub fn snow_training_exposure(
    travel_minutes: u32,
    snow_cover_bps: u16,
    road_discount_bps: u16,
) -> (u32, u32) {
    let discounted = u64::from(travel_minutes)
        * u64::from(10_000u16.saturating_sub(road_discount_bps.min(10_000)))
        / 10_000;
    let snow_minutes = discounted * u64::from(snow_cover_bps.min(10_000)) / 10_000;
    ((discounted - snow_minutes) as u32, snow_minutes as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_is_stable_within_an_interval_and_cells_are_coarse() {
        let a = weather_at(42, 123_456, 53_551_000, 9_993_000, 10);
        assert_eq!(a, weather_at(42, 123_456, 53_551_000, 9_993_000, 10));
        assert_eq!(a, weather_at(42, 123_457, 53_599_999, 9_999_999, 10));
        assert_eq!(a.rules_version, WEATHER_RULES_VERSION);
    }

    #[test]
    fn epoch_start_samples_interval_zero_once() {
        let snapshot = weather_at(42, 0, 53_000_000, 10_000_000, 0);
        let sample = interval_weather(42, 0, snapshot.cell_latitude, snapshot.cell_longitude, 0);
        let (moisture, snow) = advance_ground(
            0,
            0,
            sample,
            temperature_deci_c(0, snapshot.cell_latitude, 0),
        );
        assert_eq!(snapshot.ground_moisture_bps, moisture.min(10_000) as u16);
        assert_eq!(snapshot.snow_cover_bps, snow.min(10_000) as u16);
    }

    #[test]
    fn elevation_and_season_control_rain_versus_snow() {
        let winter_interval = 30 * 4;
        let summer_interval = 180 * 4;
        assert!(
            temperature_deci_c(winter_interval, 212, 0)
                < temperature_deci_c(summer_interval, 212, 0)
        );
        assert!(
            temperature_deci_c(summer_interval, 212, 2_000)
                < temperature_deci_c(summer_interval, 212, 0)
        );
        let mut found = false;
        for interval in 0..1_460 {
            let low = interval_weather(7, interval, 212, 40, 0);
            let high = interval_weather(7, interval, 212, 40, 2_000);
            if low.precipitation == Precipitation::Rain && high.precipitation == Precipitation::Snow
            {
                found = true;
                break;
            }
        }
        assert!(found);
    }

    #[test]
    fn moisture_and_snow_are_bounded_and_have_memory() {
        for minute in (0..525_600).step_by(360) {
            let sample = weather_at(19, minute, 53_500_000, 10_000_000, 80);
            assert!(sample.ground_moisture_bps <= 10_000);
            assert!(sample.snow_cover_bps <= 10_000);
        }
    }

    #[test]
    fn rain_drains_and_snow_accumulates_then_melts() {
        let rain = IntervalWeather {
            precipitation: Precipitation::Rain,
            intensity_bps: 9_000,
        };
        let clear = IntervalWeather {
            precipitation: Precipitation::Clear,
            intensity_bps: 0,
        };
        let snowfall = IntervalWeather {
            precipitation: Precipitation::Snow,
            intensity_bps: 9_000,
        };
        let (wet, _) = advance_ground(0, 0, rain, 50);
        let (drained, _) = advance_ground(wet, 0, clear, 50);
        assert!(wet > 0 && drained < wet);
        let (_, first_snow) = advance_ground(0, 0, snowfall, -20);
        let (_, accumulated) = advance_ground(0, first_snow, snowfall, -20);
        let (_, melted) = advance_ground(0, accumulated, clear, 50);
        assert!(accumulated > first_snow);
        assert!(melted < accumulated);
    }

    #[test]
    fn snow_overlay_and_training_are_monotonic_and_keep_underlying_exposure() {
        assert_eq!(snow_overlay_check(3_000, 5_000, 0), 3_000);
        assert!(snow_overlay_check(3_000, 5_000, 8_000) > 3_000);
        let (underlying, snow) = snow_training_exposure(600, 8_000, 2_500);
        assert_eq!(underlying, 90);
        assert_eq!(snow, 360);
        let chunks = (0..6)
            .map(|_| snow_training_exposure(100, 8_000, 2_500))
            .fold((0, 0), |sum, value| (sum.0 + value.0, sum.1 + value.1));
        assert_eq!((underlying, snow), chunks);
    }
}
